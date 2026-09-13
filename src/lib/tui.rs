const HR_CHAR: &str = "-";
/// Wide enough to underline the command list and no wider. 50 was arbitrary and
/// overhung the longest line by 14 characters, which wrapped in a narrow pane.
const HR_WIDTH: usize = 36;

/// Prints the startup banner with available commands.
pub fn welcome() {
    println!("Welcome to chickadee");
    println!("Try the following commands");
    hr();
    println!("{}", command_hint());
    hr();
}

/// Prints a horizontal rule.
pub fn hr() {
    println!("{}", HR_CHAR.repeat(HR_WIDTH));
}

/// Prints the list of available commands with aliases.
pub fn command_hint() -> &'static str {
    r"set <key> <value>    (alias: s)
get <key>            (alias: g)
delete <key>         (alias: d, del)
quit                 (alias: q, exit)"
}
