use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

use crate::agent::{
    AgentActivity, AgentSession, AgentSessionManager, Task, TaskAssignParams, TaskUpdateParams,
};

/// Auto-dispatch loop: matches ready tasks to idle workers every tick.
pub async fn run(
    agent_manager: Arc<AgentSessionManager>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    tracing::info!("dispatch loop started (5s tick)");

    loop {
        tokio::select! {
            _ = interval.tick() => {
                tick(&agent_manager);
            }
            _ = shutdown_rx.changed() => {
                tracing::info!("dispatch loop shutting down");
                break;
            }
        }
    }
}

fn tick(mgr: &AgentSessionManager) {
    // Only run if orchestration is enabled with auto_dispatch
    let enabled = mgr.get_config("enabled").unwrap_or_else(|| "false".into());
    let auto_dispatch = mgr.get_config("auto_dispatch").unwrap_or_else(|| "false".into());
    if enabled != "true" || auto_dispatch != "true" {
        return;
    }

    let leader_id = mgr.get_config("leader_id");
    let ready_tasks = mgr.tasks_ready();
    let idle_agents = mgr.agents_idle();

    // Filter out leader agent from workers
    let idle_workers: Vec<&AgentSession> = idle_agents
        .iter()
        .filter(|a| {
            if let Some(ref lid) = leader_id {
                a.id != *lid
            } else {
                true
            }
        })
        .collect();

    if ready_tasks.is_empty() || idle_workers.is_empty() {
        return;
    }

    tracing::info!(
        "dispatch tick: {} ready task(s), {} idle worker(s)",
        ready_tasks.len(),
        idle_workers.len()
    );

    for (task, worker) in ready_tasks.iter().zip(idle_workers.iter()) {
        if let Err(e) = dispatch_task(mgr, task, worker, &leader_id) {
            tracing::warn!("dispatch failed for task {}: {e}", task.id);
        }
    }

    // Check for stale heartbeats (agents that haven't heartbeated in 120s)
    check_stale_agents(mgr);
}

fn dispatch_task(
    mgr: &AgentSessionManager,
    task: &Task,
    worker: &AgentSession,
    _leader_id: &Option<String>,
) -> Result<(), String> {
    // 1. Assign task (pending → assigned)
    mgr.task_assign(TaskAssignParams {
        task_id: task.id.clone(),
        agent_id: worker.id.clone(),
    })?;

    // 2. Transition to in_progress (assigned → in_progress)
    mgr.task_update(TaskUpdateParams {
        id: task.id.clone(),
        title: None,
        description: None,
        status: Some("in_progress".into()),
        priority: None,
        assignee: None,
    })?;

    // 3. Set agent to busy
    mgr.set_activity(&worker.id, AgentActivity::Busy, Some(&task.id))?;

    // 4. Format and inject task instruction via PTY
    let instruction = format_task_instruction(task, &worker.worktree_path);
    mgr.enqueue_input(&worker.id, &instruction)?;

    tracing::info!(
        "dispatched task {} ({}) to worker {} ({})",
        task.id,
        task.title,
        worker.id,
        worker.name
    );

    Ok(())
}

fn format_task_instruction(task: &Task, worktree_path: &str) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "[CMUX-TASK] task_id={} title=\"{}\"",
        task.id, task.title
    ));
    if let Some(ref desc) = task.description {
        lines.push(format!("Description: {desc}"));
    }
    lines.push(format!("Worktree: {worktree_path}"));
    lines.push(String::new());
    lines.push(format!(
        "When done: cmux-ctl task complete {} --result \"summary of what was done\"",
        task.id
    ));
    lines.push(format!(
        "If stuck: cmux-ctl task fail {} --error \"reason\"",
        task.id
    ));
    lines.join("\n")
}

fn check_stale_agents(mgr: &AgentSessionManager) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let stale_threshold_ms = 120_000; // 2 minutes

    // Get all busy agents and check their heartbeats
    let agents = mgr.list(false);
    for agent in agents {
        if agent.activity == AgentActivity::Busy
            && agent.last_heartbeat_ms > 0
            && now - agent.last_heartbeat_ms > stale_threshold_ms
        {
            tracing::warn!(
                "agent {} ({}) has stale heartbeat (last: {}ms ago), marking idle",
                agent.id,
                agent.name,
                now - agent.last_heartbeat_ms
            );

            // Reset agent to idle
            let _ = mgr.set_activity(&agent.id, AgentActivity::Idle, None);

            // If agent had a current task, reset it to pending for re-dispatch
            if let Some(ref task_id) = agent.current_task_id {
                let _ = mgr.task_update(TaskUpdateParams {
                    id: task_id.clone(),
                    title: None,
                    description: None,
                    status: None,
                    priority: None,
                    assignee: None,
                });
                tracing::info!(
                    "task {} from stale agent {} available for re-dispatch",
                    task_id,
                    agent.id
                );
            }
        }
    }
}
