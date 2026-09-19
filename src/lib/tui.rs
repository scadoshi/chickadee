const HR_CHAR: &str = "-";
/// Matches the longest command-hint line so the rule does not wrap in a narrow pane.
const HR_WIDTH: usize = 36;

/// Prints the banner and command list.
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

/// The command list with aliases.
pub fn command_hint() -> &'static str {
    r"set <key> <value>    (alias: s)
get <key>            (alias: g)
delete <key>         (alias: d, del)
quit                 (alias: q, exit)"
}
