mod rpc;

use rpc::RpcClient;
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        return ExitCode::from(2);
    }

    let mut client = match RpcClient::connect() {
        Some(c) => c,
        None => {
            eprintln!("cmux-ctl: cannot connect to term-meshd daemon");
            return ExitCode::from(1);
        }
    };

    let result = match args[1].as_str() {
        "ping" => cmd_ping(&mut client),
        "task" => cmd_task(&mut client, &args[2..]),
        "agent" => cmd_agent(&mut client, &args[2..]),
        "message" | "msg" => cmd_message(&mut client, &args[2..]),
        "orchestration" | "orch" => cmd_orchestration(&mut client, &args[2..]),
        "status" => cmd_orchestration(&mut client, &["status".to_string()]),
        "help" | "--help" | "-h" => {
            print_usage();
            return ExitCode::SUCCESS;
        }
        other => {
            eprintln!("cmux-ctl: unknown command '{other}'");
            print_usage();
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("cmux-ctl: {e}");
            ExitCode::from(1)
        }
    }
}

// ---------------------------------------------------------------------------
// ping
// ---------------------------------------------------------------------------

fn cmd_ping(client: &mut RpcClient) -> Result<String, String> {
    let result = client.call("ping", serde_json::json!({}))?;
    Ok(format!("{result}"))
}

// ---------------------------------------------------------------------------
// task
// ---------------------------------------------------------------------------

fn cmd_task(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("usage: cmux-ctl task <create|complete|fail|list|get|log>".into());
    }

    match args[0].as_str() {
        "create" => task_create(client, &args[1..]),
        "complete" => task_complete(client, &args[1..]),
        "fail" => task_fail(client, &args[1..]),
        "list" => task_list(client, &args[1..]),
        "get" => task_get(client, &args[1..]),
        "log" => task_log(client, &args[1..]),
        other => Err(format!("unknown task subcommand: {other}")),
    }
}

fn task_create(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("usage: cmux-ctl task create <title> [--desc ...] [--deps id1,id2] [--priority N]".into());
    }

    let title = &args[0];
    let mut desc: Option<String> = None;
    let mut deps: Option<Vec<String>> = None;
    let mut priority: Option<i32> = None;
    let mut created_by = rpc::detect_agent_id();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--desc" | "--description" => {
                i += 1;
                desc = args.get(i).cloned();
            }
            "--deps" => {
                i += 1;
                if let Some(val) = args.get(i) {
                    deps = Some(val.split(',').map(|s| s.trim().to_string()).collect());
                }
            }
            "--priority" => {
                i += 1;
                priority = args.get(i).and_then(|v| v.parse().ok());
            }
            "--created-by" => {
                i += 1;
                created_by = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let mut params = serde_json::json!({ "title": title });
    if let Some(d) = desc {
        params["description"] = serde_json::json!(d);
    }
    if let Some(d) = deps {
        params["deps"] = serde_json::json!(d);
    }
    if let Some(p) = priority {
        params["priority"] = serde_json::json!(p);
    }
    if let Some(c) = created_by {
        params["created_by"] = serde_json::json!(c);
    }

    let result = client.call("task.create", params)?;
    Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
}

fn task_complete(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("usage: cmux-ctl task complete <task-id> [--result ...]".into());
    }

    let task_id = &args[0];
    let agent_id = rpc::detect_agent_id();
    let mut result_text: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        if args[i] == "--result" {
            i += 1;
            result_text = args.get(i).cloned();
        }
        i += 1;
    }

    let mut params = serde_json::json!({ "task_id": task_id });
    if let Some(a) = agent_id {
        params["agent_id"] = serde_json::json!(a);
    }
    if let Some(r) = result_text {
        params["result"] = serde_json::json!(r);
    }

    let result = client.call("task.complete", params)?;
    Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
}

fn task_fail(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("usage: cmux-ctl task fail <task-id> [--error ...]".into());
    }

    let task_id = &args[0];
    let agent_id = rpc::detect_agent_id();
    let mut error_text: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        if args[i] == "--error" {
            i += 1;
            error_text = args.get(i).cloned();
        }
        i += 1;
    }

    let mut params = serde_json::json!({ "task_id": task_id });
    if let Some(a) = agent_id {
        params["agent_id"] = serde_json::json!(a);
    }
    if let Some(e) = error_text {
        params["error"] = serde_json::json!(e);
    }

    let result = client.call("task.fail", params)?;
    Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
}

fn task_list(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    let mut status: Option<String> = None;
    let mut assignee: Option<String> = None;
    let mut ready = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--status" => {
                i += 1;
                status = args.get(i).cloned();
            }
            "--assignee" => {
                i += 1;
                assignee = args.get(i).cloned();
            }
            "--ready" => {
                ready = true;
            }
            _ => {}
        }
        i += 1;
    }

    if ready {
        let result = client.call("task.ready_list", serde_json::json!({}))?;
        return Ok(serde_json::to_string_pretty(&result).unwrap_or_default());
    }

    let mut params = serde_json::json!({});
    if let Some(s) = status {
        params["status"] = serde_json::json!(s);
    }
    if let Some(a) = assignee {
        params["assignee"] = serde_json::json!(a);
    }

    let result = client.call("task.list", params)?;
    Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
}

fn task_get(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("usage: cmux-ctl task get <task-id>".into());
    }
    let result = client.call("task.get", serde_json::json!({ "id": args[0] }))?;
    Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
}

fn task_log(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("usage: cmux-ctl task log <task-id> [message]".into());
    }

    let task_id = &args[0];

    // If there's a message, add a log entry
    if args.len() > 1 && !args[1].starts_with("--") {
        let message = args[1..].join(" ");
        let agent_id = rpc::detect_agent_id();
        let mut params = serde_json::json!({
            "task_id": task_id,
            "message": message,
        });
        if let Some(a) = agent_id {
            params["agent_id"] = serde_json::json!(a);
        }
        let result = client.call("task.log_add", params)?;
        return Ok(serde_json::to_string_pretty(&result).unwrap_or_default());
    }

    // Otherwise list log entries
    let result = client.call("task.log", serde_json::json!({ "task_id": task_id }))?;
    Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
}

// ---------------------------------------------------------------------------
// agent
// ---------------------------------------------------------------------------

fn cmd_agent(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("usage: cmux-ctl agent <list|spawn|terminate|activity|heartbeat>".into());
    }

    match args[0].as_str() {
        "list" => agent_list(client, &args[1..]),
        "spawn" => agent_spawn(client, &args[1..]),
        "terminate" => agent_terminate(client, &args[1..]),
        "activity" => agent_activity(client, &args[1..]),
        "heartbeat" => agent_heartbeat(client),
        other => Err(format!("unknown agent subcommand: {other}")),
    }
}

fn agent_list(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    let idle_only = args.iter().any(|a| a == "--idle");
    if idle_only {
        let result = client.call("agent.idle_list", serde_json::json!({}))?;
        return Ok(serde_json::to_string_pretty(&result).unwrap_or_default());
    }

    let include_terminated = args.iter().any(|a| a == "--all");
    let result = client.call(
        "agent.list",
        serde_json::json!({ "include_terminated": include_terminated }),
    )?;
    Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
}

fn agent_spawn(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("usage: cmux-ctl agent spawn <repo-path> [--count N] [--command ...]".into());
    }

    let repo_path = &args[0];
    let mut count: usize = 1;
    let mut name: Option<String> = None;
    let mut command: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--count" => {
                i += 1;
                count = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(1);
            }
            "--name" => {
                i += 1;
                name = args.get(i).cloned();
            }
            "--command" => {
                i += 1;
                command = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let mut params = serde_json::json!({
        "repo_path": repo_path,
        "count": count,
    });
    if let Some(n) = name {
        params["name"] = serde_json::json!(n);
    }
    if let Some(c) = command {
        params["command"] = serde_json::json!(c);
    }

    let result = client.call("agent.spawn", params)?;
    Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
}

fn agent_terminate(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("usage: cmux-ctl agent terminate <agent-id>".into());
    }
    let force = args.iter().any(|a| a == "--force");
    let result = client.call(
        "agent.terminate",
        serde_json::json!({ "id": args[0], "force": force }),
    )?;
    Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
}

fn agent_activity(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    if args.len() < 2 {
        return Err("usage: cmux-ctl agent activity <agent-id> <idle|busy> [--task <task-id>]".into());
    }

    let session_id = &args[0];
    let activity = &args[1];
    let mut task_id: Option<String> = None;

    let mut i = 2;
    while i < args.len() {
        if args[i] == "--task" {
            i += 1;
            task_id = args.get(i).cloned();
        }
        i += 1;
    }

    let mut params = serde_json::json!({
        "session_id": session_id,
        "activity": activity,
    });
    if let Some(t) = task_id {
        params["task_id"] = serde_json::json!(t);
    }

    let result = client.call("agent.set_activity", params)?;
    Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
}

fn agent_heartbeat(client: &mut RpcClient) -> Result<String, String> {
    let agent_id = rpc::detect_agent_id()
        .ok_or_else(|| "cannot detect agent ID (not in an agent worktree)".to_string())?;
    client.call("agent.heartbeat", serde_json::json!({ "session_id": agent_id }))?;
    Ok("ok".into())
}

// ---------------------------------------------------------------------------
// message
// ---------------------------------------------------------------------------

fn cmd_message(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("usage: cmux-ctl message <send|list|ack>".into());
    }

    match args[0].as_str() {
        "send" => msg_send(client, &args[1..]),
        "list" => msg_list(client, &args[1..]),
        "ack" => msg_ack(client, &args[1..]),
        other => Err(format!("unknown message subcommand: {other}")),
    }
}

fn msg_send(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    if args.len() < 2 {
        return Err("usage: cmux-ctl message send <to-agent-id> <content>".into());
    }

    let to_agent = &args[0];
    let content = args[1..].join(" ");
    let from_agent = rpc::detect_agent_id();

    let mut params = serde_json::json!({
        "to_agent": to_agent,
        "content": content,
    });
    if let Some(f) = from_agent {
        params["from_agent"] = serde_json::json!(f);
    }

    let result = client.call("message.send", params)?;
    Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
}

fn msg_list(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("usage: cmux-ctl message list <agent-id> [--unread]".into());
    }

    let agent_id = &args[0];
    let unread_only = args.iter().any(|a| a == "--unread");

    let result = client.call(
        "message.list",
        serde_json::json!({ "agent_id": agent_id, "unread_only": unread_only }),
    )?;
    Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
}

fn msg_ack(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("usage: cmux-ctl message ack <msg-id> [msg-id2 ...]".into());
    }

    let ids: Vec<i64> = args
        .iter()
        .filter_map(|a| a.parse().ok())
        .collect();

    let result = client.call("message.ack", serde_json::json!({ "message_ids": ids }))?;
    Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
}

// ---------------------------------------------------------------------------
// orchestration
// ---------------------------------------------------------------------------

fn cmd_orchestration(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    let sub = if args.is_empty() { "status" } else { args[0].as_str() };

    match sub {
        "start" => orch_start(client, &args[1..]),
        "stop" => {
            let result = client.call("orchestration.stop", serde_json::json!({}))?;
            Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "status" => {
            let result = client.call("orchestration.status", serde_json::json!({}))?;
            Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "goal" => orch_goal(client, &args[1..]),
        other => Err(format!("unknown orchestration subcommand: {other}")),
    }
}

fn orch_start(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    let mut leader_agent_id: Option<String> = None;
    let mut auto_dispatch = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--leader" => {
                i += 1;
                leader_agent_id = args.get(i).cloned();
            }
            "--auto-dispatch" => {
                auto_dispatch = true;
            }
            _ => {}
        }
        i += 1;
    }

    let mut params = serde_json::json!({ "auto_dispatch": auto_dispatch });
    if let Some(l) = leader_agent_id {
        params["leader_agent_id"] = serde_json::json!(l);
    }

    let result = client.call("orchestration.start", params)?;
    Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
}

fn orch_goal(client: &mut RpcClient, args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("usage: cmux-ctl orchestration goal <description>".into());
    }

    let description = args.join(" ");
    let result = client.call(
        "orchestration.goal",
        serde_json::json!({ "description": description }),
    )?;
    Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
}

// ---------------------------------------------------------------------------
// help
// ---------------------------------------------------------------------------

fn print_usage() {
    eprintln!("cmux-ctl — agent orchestration CLI for cmux");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  cmux-ctl task create <title> [--desc ...] [--deps id1,id2] [--priority N]");
    eprintln!("  cmux-ctl task complete <task-id> [--result <summary>]");
    eprintln!("  cmux-ctl task fail <task-id> [--error <message>]");
    eprintln!("  cmux-ctl task list [--status <s>] [--ready]");
    eprintln!("  cmux-ctl task get <task-id>");
    eprintln!("  cmux-ctl task log <task-id> [message]");
    eprintln!();
    eprintln!("  cmux-ctl agent list [--idle] [--all]");
    eprintln!("  cmux-ctl agent spawn <repo> [--count N] [--command <cmd>]");
    eprintln!("  cmux-ctl agent terminate <agent-id> [--force]");
    eprintln!("  cmux-ctl agent activity <agent-id> <idle|busy> [--task <task-id>]");
    eprintln!("  cmux-ctl agent heartbeat");
    eprintln!();
    eprintln!("  cmux-ctl message send <to-agent-id> <content>");
    eprintln!("  cmux-ctl message list <agent-id> [--unread]");
    eprintln!("  cmux-ctl message ack <msg-id>...");
    eprintln!();
    eprintln!("  cmux-ctl orchestration start [--leader <id>] [--auto-dispatch]");
    eprintln!("  cmux-ctl orchestration stop");
    eprintln!("  cmux-ctl orchestration status");
    eprintln!("  cmux-ctl orchestration goal <description>");
    eprintln!();
    eprintln!("  cmux-ctl ping");
    eprintln!("  cmux-ctl help");
    eprintln!();
    eprintln!("Agent ID is auto-detected from .cmux/agent.json in the worktree.");
}
