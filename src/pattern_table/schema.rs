use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One element of a compiled command pattern.
#[derive(Debug, Clone, PartialEq)]
pub enum PatternItem {
    /// A word that must appear exactly, such as `is greater than`.
    Literal(String),
    /// A quoted string that must match exactly, written `"like this"` in the
    /// pattern.
    Quoted(String),
    /// A `<name>` slot that matches exactly one token. `quoted` is set when the
    /// slot was written inside quotes in the pattern, as in `teach "<pattern>"`.
    Placeholder { name: String, quoted: bool },
    /// A `<name...>` slot that swallows the rest of the line. Used for list
    /// literals and for the arguments of `ask the system for`.
    RestPlaceholder { name: String },
    /// Anything written inside `[ ]` in the pattern: the line may end with
    /// this or leave it out. `teach "<pattern>" [:]` accepts both
    /// `teach "hi":` and `teach "hi"`. A `:` reads as a symbol, so this item
    /// compares on the token's own text rather than asking for a word.
    OptionalLiteral(String),
}

/// A pattern string such as `set <name> to <value>` split into items.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompiledPattern {
    pub items: Vec<PatternItem>,
    /// Placeholder names in the order they appear, which is the order handlers
    /// receive their arguments in.
    pub placeholders: Vec<String>,
}

impl CompiledPattern {
    /// Compile a pattern string. Returns an error for unbalanced quotes or
    /// angle brackets so that a typo in the table is caught immediately.
    pub fn compile(pattern: &str) -> Result<Self, String> {
        let chars: Vec<char> = pattern.chars().collect();
        let mut index = 0usize;
        let mut items: Vec<PatternItem> = Vec::new();
        let mut placeholders: Vec<String> = Vec::new();

        while index < chars.len() {
            let ch = chars[index];
            if ch.is_whitespace() {
                index += 1;
                continue;
            }

            if ch == '"' {
                index += 1;
                let mut text = String::new();
                let mut closed = false;
                while index < chars.len() {
                    if chars[index] == '"' {
                        index += 1;
                        closed = true;
                        break;
                    }
                    text.push(chars[index]);
                    index += 1;
                }
                if !closed {
                    return Err(format!("unclosed quote in pattern \"{pattern}\""));
                }
                // `"<name>"` is a slot that has to be quoted, rather than a
                // fixed word. Without this `teach "<pattern>"` would only ever
                // match the literal text `<pattern>`.
                let inner = text.trim();
                if inner.starts_with('<') && inner.ends_with('>') {
                    let mut name = inner[1..inner.len() - 1].to_string();
                    let rest = name.ends_with("..");
                    if rest {
                        name = name.trim_end_matches('.').to_string();
                    }
                    if name.is_empty() {
                        return Err(format!("empty placeholder in pattern \"{pattern}\""));
                    }
                    if rest {
                        items.push(PatternItem::RestPlaceholder { name: name.clone() });
                    } else {
                        items.push(PatternItem::Placeholder {
                            name: name.clone(),
                            quoted: true,
                        });
                    }
                    placeholders.push(name);
                } else {
                    items.push(PatternItem::Quoted(text));
                }
                continue;
            }

            if ch == '<' {
                index += 1;
                let mut name = String::new();
                let mut closed = false;
                while index < chars.len() {
                    if chars[index] == '>' {
                        index += 1;
                        closed = true;
                        break;
                    }
                    name.push(chars[index]);
                    index += 1;
                }
                if !closed {
                    return Err(format!("unclosed < in pattern \"{pattern}\""));
                }
                let rest = name.ends_with("..");
                if rest {
                    name = name.trim_end_matches('.').to_string();
                }
                if name.is_empty() {
                    return Err(format!("empty placeholder in pattern \"{pattern}\""));
                }
                if rest {
                    items.push(PatternItem::RestPlaceholder { name: name.clone() });
                } else {
                    items.push(PatternItem::Placeholder {
                        name: name.clone(),
                        quoted: false,
                    });
                }
                placeholders.push(name);
                continue;
            }

            // `[...]` is an optional tail: the line may end with these words or
            // without them. `[:]` lets `teach "<pattern>":` and
            // `teach "<pattern>"` be the same command.
            if ch == '[' {
                index += 1;
                let mut text = String::new();
                let mut closed = false;
                while index < chars.len() {
                    if chars[index] == ']' {
                        index += 1;
                        closed = true;
                        break;
                    }
                    text.push(chars[index]);
                    index += 1;
                }
                if !closed {
                    return Err(format!("unclosed [ in pattern \"{pattern}\""));
                }
                let mut found = false;
                for word in text.split_whitespace() {
                    found = true;
                    items.push(PatternItem::OptionalLiteral(word.to_string()));
                }
                if !found {
                    return Err(format!("empty [ ] in pattern \"{pattern}\""));
                }
                continue;
            }

            let mut word = String::new();
            while index < chars.len() {
                let c = chars[index];
                if c.is_whitespace() || c == '"' || c == '<' {
                    break;
                }
                word.push(c);
                index += 1;
            }
            items.push(PatternItem::Literal(word));
        }

        if items.is_empty() {
            return Err("pattern is empty".to_string());
        }

        Ok(Self {
            items,
            placeholders,
        })
    }

    /// The words that must appear literally at the start of the pattern. This is
    /// the name the command is known by, so `greet <name>` is the command
    /// `greet`.
    pub fn command_name(&self) -> String {
        self.items
            .iter()
            .take_while(|item| matches!(item, PatternItem::Literal(_)))
            .map(|item| match item {
                PatternItem::Literal(text) => text.as_str(),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn has_rest_placeholder(&self) -> bool {
        self.items
            .iter()
            .any(|item| matches!(item, PatternItem::RestPlaceholder { .. }))
    }
}

/// What a command does to the shape of the program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// An ordinary statement.
    #[default]
    Command,
    /// Opens a block that runs until `end`.
    BlockStart,
    /// Closes the innermost block.
    BlockEnd,
    /// Starts the else branch of an `if` block.
    Otherwise,
}

/// One row of `builtins.toml`. The table is the single source of truth: the
/// validator, the interpreter, `help` and the generated documentation all read
/// `pattern`, `handler` and `role` from here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandPattern {
    pub name: String,
    pub module: String,
    pub pattern: String,
    /// Name of the Rust function that runs this command, for example
    /// `core::print`.
    pub handler: String,
    pub description: String,
    pub usage: String,
    pub example: String,
    pub error_msg: String,
    #[serde(default)]
    pub role: Role,
    /// True for words the REPL answers itself, such as `help` and `exit`.
    #[serde(default)]
    pub repl_only: bool,

    /// Parsed form of `pattern`, filled in by `PatternTable::new` and never
    /// written back to TOML.
    #[serde(skip, default)]
    pub compiled: CompiledPattern,
}

impl CommandPattern {
    pub fn compile(mut self) -> Result<Self, String> {
        self.compiled = CompiledPattern::compile(&self.pattern)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternTableData {
    #[serde(default)]
    pub package: BTreeMap<String, String>,
    pub commands: Vec<CommandPattern>,
}

/// A loaded pattern table. Commands are kept in a `Vec` in a stable order so
/// that generated documentation does not change between runs.
#[derive(Debug, Clone)]
pub struct PatternTable {
    commands: Vec<CommandPattern>,
    /// Module names in the order they first appear in the table.
    modules: Vec<String>,
}

impl PatternTable {
    pub fn new(data: PatternTableData) -> Result<Self, String> {
        let mut commands = Vec::with_capacity(data.commands.len());
        let mut modules: Vec<String> = Vec::new();

        for command in data.commands {
            if !modules.contains(&command.module) {
                modules.push(command.module.clone());
            }
            commands.push(command.compile()?);
        }

        // Sorting by (module, name) makes the table, the documentation and the
        // error suggestions deterministic.
        commands.sort_by(|a, b| {
            a.module
                .cmp(&b.module)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.pattern.cmp(&b.pattern))
        });

        Ok(Self { commands, modules })
    }

    pub fn commands(&self) -> &[CommandPattern] {
        &self.commands
    }

    pub fn modules(&self) -> &[String] {
        &self.modules
    }

    pub fn get(&self, name: &str) -> Option<&CommandPattern> {
        self.commands.iter().find(|c| c.name == name)
    }

    pub fn exists(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    pub fn in_module(&self, module: &str) -> Vec<&CommandPattern> {
        self.commands
            .iter()
            .filter(|c| c.module == module)
            .collect()
    }

    /// Every module command, used to answer "did you mean `use math`?".
    pub fn module_of_command(&self, name: &str) -> Option<&str> {
        self.commands
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.module.as_str())
    }

    /// Add a command at run time, as `teach` does. Returns `true` when it
    /// replaced an existing command, which the caller reports as a warning.
    /// Nothing is ever written back to `builtins.toml`.
    pub fn insert(&mut self, command: CommandPattern) -> Result<bool, String> {
        let compiled = CompiledPattern::compile(&command.pattern)?;
        let mut command = command;
        command.compiled = compiled;

        let key = command.compiled.command_name();
        if !self.modules.contains(&command.module) {
            self.modules.push(command.module.clone());
        }

        // Replace any earlier command that answers to the same words.
        if let Some(position) = self
            .commands
            .iter()
            .position(|c| c.compiled.command_name() == key)
        {
            let replaced = std::mem::replace(&mut self.commands[position], command);
            let still_listed = self.commands.iter().any(|c| c.module == replaced.module);
            if !still_listed {
                self.modules.retain(|m| m != &replaced.module);
            }
            return Ok(true);
        }

        self.commands.push(command);
        self.commands.sort_by(|a, b| {
            a.module
                .cmp(&b.module)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.pattern.cmp(&b.pattern))
        });
        Ok(false)
    }
}
