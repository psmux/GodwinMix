//! Tools for the methods whose definition carries none.
//!
//! Most methods name their tool where they are registered, with `.tool()`.
//! The station's methods (`show.*`, `channel.*`, the governor, the project
//! file) were registered without one, and some of them are written by other
//! hands than the tool list. This table binds them from one place, by method
//! name, so a binding can be written ahead of the method it names and takes
//! effect the day that method is registered.

mod shows;
mod station;

use crate::method::{McpBinding, Registry, Tier};

/// One row of the table.
pub struct Binding {
    pub method: &'static str,
    pub tool: &'static str,
    pub tier: Tier,
    pub description: &'static str,
}

/// Every row, the shows first.
pub fn table() -> impl Iterator<Item = &'static Binding> {
    shows::BINDINGS.iter().chain(station::BINDINGS.iter())
}

/// Bind every row whose method is registered and has no tool yet.
pub fn bind<C>(reg: &mut Registry<C>) {
    for row in table() {
        let binding = McpBinding { tool: row.tool, tier: row.tier, description: row.description };
        reg.bind(row.method, binding);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_is_a_snake_case_tool_with_a_manual() {
        for row in table() {
            assert!(row.description.len() > 80, "{} has too short a description to act on", row.tool);
            assert!(
                row.tool.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{} is not a snake case tool name",
                row.tool
            );
            assert!(row.method.contains('.'), "{} is not a method name", row.method);
        }
    }

    #[test]
    fn no_two_rows_share_a_method_or_a_tool() {
        let rows: Vec<&Binding> = table().collect();
        for (i, a) in rows.iter().enumerate() {
            for b in &rows[i + 1..] {
                assert_ne!(a.method, b.method);
                assert_ne!(a.tool, b.tool);
            }
        }
    }
}
