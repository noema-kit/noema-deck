//! The `usage` command.

use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{DateTime, Days, Local, NaiveDate};
use noema_deck_core::{
    agent::Homes,
    usage::{Asker, Ledger, Tokens},
};

/// Prints the tokens each agent used in the last `days` days, by model.
pub fn print(days: u64) -> Result<()> {
    let report = Ledger::default().report(&Homes::find()?)?;
    let today = Local::now().date_naive();
    let first = today.checked_sub_days(Days::new(days.max(1) - 1)).unwrap_or(NaiveDate::MIN);
    let first = first.format("%Y-%m-%d").to_string();
    let mut used = BTreeMap::<(String, &str), Tokens>::new();
    for day in report.days.iter().filter(|day| day.day >= first) {
        *used.entry((day.agent.to_string(), &day.model)).or_default() += day.tokens;
    }
    if used.is_empty() {
        println!("No tokens used since {first}.");
    }
    let width = used.keys().map(|(_, model)| model.len()).max().unwrap_or(0);
    let mut all = Tokens::default();
    println!(
        "{:11}  {:width$}  {:>8}  {:>8}  {:>8}  {:>8}  {:>8}  {:>7}",
        "Agent", "Model", "Total", "Input", "Cached", "Written", "Output", "Replies"
    );
    for ((agent, model), tokens) in &used {
        all += *tokens;
        row(agent, model, width, tokens);
    }
    row("All", "", width, &all);
    Ok(())
}

/// Asks each agent how much of its plan is used, and prints each window.
pub fn limits() -> Result<()> {
    let homes = Homes::find()?;
    let codex = Ledger::default().codex_plan(&homes)?;
    let [claude, antigravity] = Asker::default().plans(&homes, true);
    for plan in [claude, codex, antigravity] {
        if plan.limits.is_empty() {
            println!("{:11}  No limits reported", plan.agent.name());
        }
        for limit in &plan.limits {
            let resets = limit.resets.and_then(|at| DateTime::from_timestamp(at, 0)).map_or_else(
                || "not started".into(),
                |at| at.with_timezone(&Local).format("resets %Y-%m-%d %H:%M").to_string(),
            );
            println!(
                "{:11}  {:28}  {:>4.0}% used  {resets}",
                plan.agent.name(),
                limit.name,
                limit.used
            );
        }
    }
    Ok(())
}

fn row(agent: &str, model: &str, width: usize, tokens: &Tokens) {
    let [total, input, cached, written, output] =
        [tokens.total(), tokens.input, tokens.cached, tokens.written, tokens.output].map(short);
    println!(
        "{agent:11}  {model:width$}  {total:>8}  {input:>8}  {cached:>8}  {written:>8}  {output:>8}  {:>7}",
        tokens.replies
    );
}

/// A count in at most four figures, such as `12.3M`.
#[expect(clippy::cast_precision_loss, reason = "Shown to three figures")]
fn short(count: u64) -> String {
    let count = count as f64;
    match [(1e9, "B"), (1e6, "M"), (1e3, "K")].into_iter().find(|(unit, _)| count >= *unit) {
        Some((unit, name)) => format!("{:.1}{name}", count / unit),
        None => format!("{count}"),
    }
}
