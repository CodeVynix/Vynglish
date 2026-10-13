pub mod schema;

pub use schema::{
    CommandPattern, CompiledPattern, PatternItem, PatternTable, PatternTableData, Role,
};

use std::sync::LazyLock;

/// The table that ships inside the binary. It is never modified: `teach` adds
/// commands to a per-session copy instead.
static BUILTINS_TOML: &str = include_str!("builtins.toml");

static BUILTINS: LazyLock<PatternTable> = LazyLock::new(|| {
    let data: PatternTableData =
        toml::from_str(BUILTINS_TOML).expect("builtins.toml is not valid TOML");
    PatternTable::new(data).expect("builtins.toml contains an invalid pattern")
});

/// A fresh copy of the built-in table. Every `Session` owns one so that `teach`
/// can grow it without affecting other sessions or the file on disk.
pub fn load_pattern_table() -> PatternTable {
    BUILTINS.clone()
}
