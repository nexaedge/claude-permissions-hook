use brush_parser::ast;

use crate::domain::ProgramName;

/// A parsed segment of a shell command, representing one program invocation.
#[derive(Debug, PartialEq)]
pub(crate) struct CommandSegment {
    pub(crate) program: ProgramName,
    pub(crate) args: Vec<String>,
}

/// Error returned when a command string cannot be parsed.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ParseError(pub String);

/// Parse a shell command string into individual command segments.
///
/// Uses brush-parser to build a proper shell AST, then walks it to extract
/// program names. Correctly handles quoting, escaping, subshells, and
/// compound commands.
///
/// Returns `Err` on parse failures so the caller can fail closed.
pub(crate) fn parse(command: &str) -> Result<Vec<CommandSegment>, ParseError> {
    if command.trim().is_empty() {
        return Ok(vec![]);
    }

    let mut parser = brush_parser::Parser::builder()
        .reader(std::io::Cursor::new(command.to_string()))
        .build();

    let program = parser
        .parse_program()
        .map_err(|e| ParseError(e.to_string()))?;

    let mut segments = Vec::new();
    visit_program(&program, &mut segments);
    Ok(segments)
}

/// Expand combined short flags into individual flags.
///
/// `-rf` → `["-r", "-f"]`. Long flags (`--force`), single short flags (`-v`),
/// positionals, bare `-`, `--`, and flags with `=` are returned unchanged.
pub(crate) fn expand_flags(arg: &str) -> Vec<String> {
    if !arg.starts_with('-')
        || arg == "-"
        || arg == "--"
        || arg.starts_with("--")
        || arg.contains('=')
    {
        return vec![arg.to_string()];
    }
    // Single short flag: -v (exactly 2 chars)
    let chars: Vec<char> = arg[1..].chars().collect();
    if chars.len() == 1 {
        return vec![arg.to_string()];
    }
    // Combined short flags: -rf → ["-r", "-f"]
    chars.iter().map(|c| format!("-{c}")).collect()
}

fn visit_program(program: &ast::Program, segments: &mut Vec<CommandSegment>) {
    for complete_command in &program.complete_commands {
        // CompleteCommand = CompoundList, CompoundList.0 = Vec<CompoundListItem>
        // CompoundListItem(AndOrList, SeparatorOperator)
        for item in &complete_command.0 {
            visit_and_or_list(&item.0, segments);
        }
    }
}

fn visit_and_or_list(list: &ast::AndOrList, segments: &mut Vec<CommandSegment>) {
    visit_pipeline(&list.first, segments);
    for and_or in &list.additional {
        match and_or {
            ast::AndOr::And(pipeline) | ast::AndOr::Or(pipeline) => {
                visit_pipeline(pipeline, segments);
            }
        }
    }
}

fn visit_pipeline(pipeline: &ast::Pipeline, segments: &mut Vec<CommandSegment>) {
    for command in &pipeline.seq {
        visit_command(command, segments);
    }
}

/// Shell builtins and utilities that transparently execute another program.
/// When these appear as the command name, the actual program being executed
/// is found in the suffix (arguments).
const TRANSPARENT_WRAPPERS: &[&str] = &["command", "env", "nohup", "exec", "builtin"];

fn visit_command(command: &ast::Command, segments: &mut Vec<CommandSegment>) {
    match command {
        ast::Command::Simple(simple) => {
            if let Some(word) = &simple.word_or_name {
                let name = word.flatten();
                if !name.is_empty() {
                    let basename = std::path::Path::new(&name)
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or(&name);

                    // If this is a transparent wrapper, extract only the wrapped program(s).
                    // Wrappers like `command`, `env`, `nohup` are shell mechanisms — the
                    // permission-relevant program is the one they launch, not the wrapper.
                    if TRANSPARENT_WRAPPERS.contains(&basename) {
                        if let Some(suffix) = &simple.suffix {
                            let unwrapped = extract_wrapped_programs(suffix, basename);
                            if !unwrapped.is_empty() {
                                segments.extend(unwrapped);
                                return;
                            }
                        }
                    }

                    // Not a wrapper (or wrapper with no arguments) — emit as-is
                    let args = extract_args_from_suffix(&simple.suffix);
                    segments.push(CommandSegment {
                        program: ProgramName::new(&name),
                        args,
                    });
                }
            }
        }
        ast::Command::Compound(compound, _) => visit_compound(compound, segments),
        ast::Command::Function(func) => visit_compound(&func.body.0, segments),
        ast::Command::ExtendedTest(_) => {} // [[ ]] doesn't execute programs
    }
}

/// Extract arguments from a command suffix, applying flag expansion.
///
/// Iterates suffix items, skips I/O redirections, assignment words, and process
/// substitutions. For each `Word` item, flattens it to a string and applies
/// `expand_flags()` to normalize combined short flags. After encountering `--`,
/// all subsequent tokens are treated as positionals (no expansion).
fn extract_args_from_suffix(suffix: &Option<ast::CommandSuffix>) -> Vec<String> {
    let Some(suffix) = suffix else {
        return vec![];
    };
    let mut args = Vec::new();
    let mut end_of_options = false;
    for item in &suffix.0 {
        if let ast::CommandPrefixOrSuffixItem::Word(word) = item {
            let text = word.flatten();
            if text == "--" {
                end_of_options = true;
                args.push(text);
                continue;
            }
            if end_of_options {
                args.push(text);
            } else {
                args.extend(expand_flags(&text));
            }
        }
        // IoRedirect, AssignmentWord, ProcessSubstitution — skip
    }
    args
}

/// Walk suffix arguments to find the actual program(s) behind wrapper commands.
///
/// Skips option flags (starting with `-`), env-style assignments (containing `=`),
/// and option arguments consumed by known flags (e.g., `env -u NAME`, `exec -a NAME`).
/// Handles nested wrappers: `env command rm` returns only `["rm"]`.
fn extract_wrapped_programs(
    suffix: &ast::CommandSuffix,
    initial_wrapper: &str,
) -> Vec<CommandSegment> {
    let mut result = Vec::new();
    let mut items = suffix.0.iter();
    let mut current_wrapper = initial_wrapper.to_string();

    loop {
        let consuming_opts = consuming_options_for(&current_wrapper);

        match find_next_program(&mut items, consuming_opts) {
            NextProgram::Single(prog) => {
                let basename = std::path::Path::new(&prog)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or(&prog);
                if TRANSPARENT_WRAPPERS.contains(&basename) {
                    // Another wrapper — update context and continue unwrapping
                    current_wrapper = basename.to_string();
                    continue;
                }
                // Found the actual target program — collect remaining items as args
                let args = collect_remaining_args(&mut items);
                result.push(CommandSegment {
                    program: ProgramName::new(&prog),
                    args,
                });
                break;
            }
            NextProgram::FromSplitString(mut segments) => {
                // Programs extracted from a -S command string.
                // Collect remaining suffix args (after the -S value) and append
                // them to the last segment — they are additional args to the
                // command specified in the split string.
                let trailing = collect_remaining_args(&mut items);
                if !trailing.is_empty() {
                    if let Some(last) = segments.last_mut() {
                        last.args.extend(trailing);
                    }
                }
                result.extend(segments);
                break;
            }
            NextProgram::None => break,
        }
    }

    result
}

/// Collect remaining suffix items as args, applying flag expansion.
///
/// Used after the target program has been identified in wrapper unwrapping.
/// Skips IoRedirect, AssignmentWord, and ProcessSubstitution items.
/// After encountering `--`, all subsequent tokens are treated as positionals.
fn collect_remaining_args<'a>(
    items: &mut impl Iterator<Item = &'a ast::CommandPrefixOrSuffixItem>,
) -> Vec<String> {
    let mut args = Vec::new();
    let mut end_of_options = false;
    for item in items {
        if let ast::CommandPrefixOrSuffixItem::Word(word) = item {
            let text = word.flatten();
            if text == "--" {
                end_of_options = true;
                args.push(text);
                continue;
            }
            if end_of_options {
                args.push(text);
            } else {
                args.extend(expand_flags(&text));
            }
        }
    }
    args
}

/// Known short/long options that consume a following separate argument for each wrapper.
///
/// Only includes options with the `--flag VALUE` form (separate argument).
/// Options using `--flag=VALUE` form are already handled by the `contains('=')` check.
///
/// Note: `-S`/`--split-string` is handled specially in `find_next_program` — its
/// consumed argument is parsed as a shell command to extract programs.
fn consuming_options_for(wrapper_basename: &str) -> &'static [&'static str] {
    match wrapper_basename {
        "env" => &[
            "-u",
            "--unset",
            "-C",
            "--chdir",
            "-S",
            "--split-string",
            "-P",
        ],
        "exec" => &["-a"],
        _ => &[],
    }
}

/// Options whose consumed argument is a shell command string that should be parsed
/// to extract the programs being executed (e.g., `env -S "echo hi"`).
const SPLIT_STRING_OPTIONS: &[&str] = &["-S", "--split-string"];

/// Extract an inline split-string payload from attached or equals forms.
///
/// Handles:
/// - `--split-string=<value>` → `Some("<value>")`
/// - `-S<value>` (attached, len > 2) → `Some("<value>")`
///
/// Returns `None` if the token is not an inline split-string form.
fn extract_inline_split_string(text: &str) -> Option<String> {
    // Long form: --split-string=<value>
    if let Some(value) = text.strip_prefix("--split-string=") {
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    // Short form: -S<value> (attached, more than just "-S")
    if let Some(value) = text.strip_prefix("-S") {
        if !value.is_empty() && !value.starts_with(' ') {
            return Some(value.to_string());
        }
    }
    None
}

/// Strip matching outer quotes from a string.
///
/// brush-parser stores Word.value as raw text including quotes. For `-S` arguments,
/// we need the unquoted content to parse as a shell command.
fn strip_outer_quotes(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Result of searching for the next program in suffix arguments.
enum NextProgram {
    /// A single program name found as a regular word.
    Single(String),
    /// Programs extracted from a `-S`/`--split-string` command string.
    FromSplitString(Vec<CommandSegment>),
    /// No more programs found.
    None,
}

/// Find the next non-option, non-assignment word in the suffix, skipping option flags
/// and their consumed arguments.
///
/// For `-S`/`--split-string` options, the consumed argument is parsed as a shell
/// command and its programs are returned as `FromSplitString`.
fn find_next_program<'a>(
    items: &mut impl Iterator<Item = &'a ast::CommandPrefixOrSuffixItem>,
    consuming_options: &[&str],
) -> NextProgram {
    let mut skip_next = false;
    let mut parse_next_as_command = false;

    for item in items.by_ref() {
        if skip_next {
            skip_next = false;

            if parse_next_as_command {
                parse_next_as_command = false;
                // Parse the -S argument as a shell command.
                // Word.value is raw text including quotes, so strip them first.
                if let ast::CommandPrefixOrSuffixItem::Word(word) = item {
                    let raw = word.flatten();
                    let unquoted = strip_outer_quotes(&raw);
                    if let Ok(segments) = parse(&unquoted) {
                        if !segments.is_empty() {
                            return NextProgram::FromSplitString(segments);
                        }
                    }
                }
            }
            continue;
        }

        if let ast::CommandPrefixOrSuffixItem::Word(word) = item {
            let text = word.flatten();

            if text.starts_with('-') {
                // Check for split-string in attached/equals forms first.
                // These embed the payload in the same token, so no skip_next needed.
                if let Some(payload) = extract_inline_split_string(&text) {
                    let unquoted = strip_outer_quotes(&payload);
                    if let Ok(segments) = parse(&unquoted) {
                        if !segments.is_empty() {
                            return NextProgram::FromSplitString(segments);
                        }
                    }
                    continue;
                }

                if consuming_options.iter().any(|opt| text == *opt) {
                    skip_next = true;
                    if SPLIT_STRING_OPTIONS.contains(&text.as_str()) {
                        parse_next_as_command = true;
                    }
                }
                continue;
            }

            if text.contains('=') {
                continue;
            }

            if !text.is_empty() {
                return NextProgram::Single(text);
            }
        }
    }

    NextProgram::None
}

fn visit_compound(command: &ast::CompoundCommand, segments: &mut Vec<CommandSegment>) {
    match command {
        ast::CompoundCommand::BraceGroup(cmd) => visit_compound_list(&cmd.list, segments),
        ast::CompoundCommand::Subshell(cmd) => visit_compound_list(&cmd.list, segments),
        ast::CompoundCommand::ForClause(cmd) => visit_compound_list(&cmd.body.list, segments),
        ast::CompoundCommand::ArithmeticForClause(cmd) => {
            visit_compound_list(&cmd.body.list, segments);
        }
        ast::CompoundCommand::WhileClause(cmd) | ast::CompoundCommand::UntilClause(cmd) => {
            visit_compound_list(&cmd.0, segments);
            visit_compound_list(&cmd.1.list, segments);
        }
        ast::CompoundCommand::IfClause(cmd) => {
            visit_compound_list(&cmd.condition, segments);
            visit_compound_list(&cmd.then, segments);
            if let Some(elses) = &cmd.elses {
                for clause in elses {
                    if let Some(condition) = &clause.condition {
                        visit_compound_list(condition, segments);
                    }
                    visit_compound_list(&clause.body, segments);
                }
            }
        }
        ast::CompoundCommand::CaseClause(cmd) => {
            for case_item in &cmd.cases {
                if let Some(body) = &case_item.cmd {
                    visit_compound_list(body, segments);
                }
            }
        }
        ast::CompoundCommand::Arithmetic(_) => {} // (( )) doesn't execute programs
    }
}

fn visit_compound_list(list: &ast::CompoundList, segments: &mut Vec<CommandSegment>) {
    for item in &list.0 {
        visit_and_or_list(&item.0, segments);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn programs(input: &str) -> Vec<String> {
        parse(input)
            .expect("parse should succeed")
            .into_iter()
            .map(|s| s.program.as_str().to_string())
            .collect()
    }

    #[test]
    fn simple_command() {
        assert_eq!(programs("git status"), vec!["git"]);
    }

    #[test]
    fn chain_with_and() {
        assert_eq!(programs("git add . && git commit"), vec!["git", "git"]);
    }

    #[test]
    fn skip_single_env_var() {
        assert_eq!(programs("ENV=val git status"), vec!["git"]);
    }

    #[test]
    fn skip_multiple_env_vars() {
        assert_eq!(programs("A=1 B=2 cargo test"), vec!["cargo"]);
    }

    #[test]
    fn pipe_operator() {
        assert_eq!(programs("ls | grep foo"), vec!["ls", "grep"]);
    }

    #[test]
    fn semicolon_operator() {
        assert_eq!(programs("cd /tmp ; rm -rf *"), vec!["cd", "rm"]);
    }

    #[test]
    fn or_operator() {
        assert_eq!(programs("git status || echo fail"), vec!["git", "echo"]);
    }

    #[test]
    fn empty_input() {
        assert_eq!(programs(""), Vec::<String>::new());
    }

    #[test]
    fn whitespace_only_input() {
        assert_eq!(programs("   "), Vec::<String>::new());
    }

    #[test]
    fn trailing_operator_is_invalid_syntax() {
        // Trailing && is invalid shell — parser returns error
        assert!(parse("git add . &&").is_err());
    }

    #[test]
    fn mixed_operators() {
        assert_eq!(
            programs("git add && cargo build | tee log"),
            vec!["git", "cargo", "tee"]
        );
    }

    #[test]
    fn quoted_operators_not_split() {
        assert_eq!(programs(r#"echo "hello && world""#), vec!["echo"]);
    }

    #[test]
    fn single_quoted_pipe_not_split() {
        assert_eq!(programs("echo 'foo|bar'"), vec!["echo"]);
    }

    #[test]
    fn subshell_commands_extracted() {
        assert_eq!(programs("(git status && echo done)"), vec!["git", "echo"]);
    }

    #[test]
    fn function_body_programs_extracted() {
        assert_eq!(programs("f(){ rm -rf /; }; f"), vec!["rm", "f"]);
    }

    #[test]
    fn case_clause_body_programs_extracted() {
        assert_eq!(
            programs("case $x in a) rm -rf /;; b) echo ok;; esac"),
            vec!["rm", "echo"]
        );
    }

    // --- Absolute/relative path extraction ---

    #[test]
    fn absolute_path_normalizes_to_basename() {
        // ProgramName::new strips the path prefix so /bin/rm → "rm"
        assert_eq!(programs("/bin/rm -rf /"), vec!["rm"]);
    }

    #[test]
    fn relative_path_normalizes_to_basename() {
        // ProgramName::new strips the path prefix so ./scripts/deploy.sh → "deploy.sh"
        assert_eq!(programs("./scripts/deploy.sh"), vec!["deploy.sh"]);
    }

    // --- Transparent wrapper unwrapping ---

    #[test]
    fn command_wrapper_unwraps_to_real_program() {
        assert_eq!(programs("command rm -rf /"), vec!["rm"]);
    }

    #[test]
    fn command_wrapper_with_option_skips_flags() {
        assert_eq!(programs("command -p rm -rf /"), vec!["rm"]);
    }

    #[test]
    fn env_wrapper_unwraps_to_real_program() {
        assert_eq!(programs("env rm -rf /"), vec!["rm"]);
    }

    #[test]
    fn env_wrapper_skips_options_and_assignments() {
        assert_eq!(programs("env -i FOO=bar rm -rf /"), vec!["rm"]);
    }

    #[test]
    fn nohup_wrapper_unwraps_to_real_program() {
        assert_eq!(programs("nohup rm -rf /"), vec!["rm"]);
    }

    #[test]
    fn exec_wrapper_unwraps_to_real_program() {
        assert_eq!(programs("exec rm -rf /"), vec!["rm"]);
    }

    #[test]
    fn builtin_wrapper_unwraps_to_real_program() {
        assert_eq!(programs("builtin echo hello"), vec!["echo"]);
    }

    #[test]
    fn nested_wrappers_fully_unwrapped() {
        assert_eq!(programs("env command rm -rf /"), vec!["rm"]);
    }

    #[test]
    fn wrapper_with_absolute_path() {
        assert_eq!(programs("/usr/bin/env rm -rf /"), vec!["rm"]);
    }

    #[test]
    fn wrapper_without_arguments_yields_wrapper_itself() {
        assert_eq!(programs("env"), vec!["env"]);
    }

    // --- Wrapper option-argument consumption ---

    #[test]
    fn env_u_skips_consumed_argument() {
        assert_eq!(programs("env -u PATH rm -rf /"), vec!["rm"]);
    }

    #[test]
    fn env_unset_long_skips_consumed_argument() {
        assert_eq!(programs("env --unset PATH rm -rf /"), vec!["rm"]);
    }

    #[test]
    fn env_chdir_skips_consumed_argument() {
        assert_eq!(programs("env -C /tmp rm -rf /"), vec!["rm"]);
    }

    #[test]
    fn env_multiple_consuming_options() {
        assert_eq!(
            programs("env --unset PATH --chdir /tmp rm -rf /"),
            vec!["rm"]
        );
    }

    #[test]
    fn exec_a_skips_consumed_argument() {
        assert_eq!(programs("exec -a fake rm -rf /"), vec!["rm"]);
    }

    #[test]
    fn env_p_skips_consumed_argument() {
        assert_eq!(programs("env -P /usr/bin rm -rf /"), vec!["rm"]);
    }

    #[test]
    fn env_split_string_parses_command() {
        assert_eq!(programs(r#"env -S "echo hi""#), vec!["echo"]);
    }

    #[test]
    fn env_split_string_long_form_parses_command() {
        assert_eq!(programs(r#"env --split-string "rm -rf /""#), vec!["rm"]);
    }

    #[test]
    fn env_split_string_with_other_options() {
        assert_eq!(programs(r#"env -i -u PATH -S "rm -rf /""#), vec!["rm"]);
    }

    // --- env -S trailing args ---

    #[test]
    fn env_split_string_trailing_args_appended() {
        // `env -S "rm" -r /` should produce program "rm" with args ["-r", "/"]
        let segs = parse(r#"env -S "rm" -r /"#).unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].program, "rm");
        assert_eq!(segs[0].args, vec!["-r", "/"]);
    }

    #[test]
    fn env_split_string_trailing_args_with_flags() {
        // `env -S "rm -rf" /tmp` should produce program "rm" with args ["-r", "-f", "/tmp"]
        let segs = parse(r#"env -S "rm -rf" /tmp"#).unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].program, "rm");
        assert_eq!(segs[0].args, vec!["-r", "-f", "/tmp"]);
    }

    #[test]
    fn env_split_string_no_trailing_args_still_works() {
        // `env -S "rm -rf /"` — all args inside split string, nothing trailing
        let segs = parse(r#"env -S "rm -rf /""#).unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].program, "rm");
        assert_eq!(segs[0].args, vec!["-r", "-f", "/"]);
    }

    // --- env -S equals/attached forms ---

    #[test]
    fn env_split_string_equals_form() {
        // `env --split-string="rm -rf /"` — equals form of --split-string
        let segs = parse(r#"env --split-string="rm -rf /""#).unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].program, "rm");
        assert_eq!(segs[0].args, vec!["-r", "-f", "/"]);
    }

    #[test]
    fn env_s_attached_form() {
        // `env -S"rm -rf /"` — attached short form of -S
        let segs = parse(r#"env -S"rm -rf /""#).unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].program, "rm");
        assert_eq!(segs[0].args, vec!["-r", "-f", "/"]);
    }

    #[test]
    fn env_split_string_equals_trailing_args() {
        // `env --split-string=rm -rf /` — equals form with trailing args
        let segs = parse("env --split-string=rm -rf /").unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].program, "rm");
        assert_eq!(segs[0].args, vec!["-r", "-f", "/"]);
    }

    // --- Arg extraction ---

    fn parse_segments(input: &str) -> Vec<CommandSegment> {
        parse(input).expect("parse should succeed")
    }

    #[test]
    fn args_simple_subcommand() {
        let segs = parse_segments("git status");
        assert_eq!(segs[0].program, "git");
        assert_eq!(segs[0].args, vec!["status"]);
    }

    #[test]
    fn args_flag_expansion() {
        let segs = parse_segments("rm -rf /");
        assert_eq!(segs[0].program, "rm");
        assert_eq!(segs[0].args, vec!["-r", "-f", "/"]);
    }

    #[test]
    fn args_long_flag_and_positionals() {
        let segs = parse_segments("git push --force origin main");
        assert_eq!(segs[0].program, "git");
        assert_eq!(segs[0].args, vec!["push", "--force", "origin", "main"]);
    }

    #[test]
    fn args_pipe_each_segment_has_own_args() {
        let segs = parse_segments("ls -la | grep foo");
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].program, "ls");
        assert_eq!(segs[0].args, vec!["-l", "-a"]);
        assert_eq!(segs[1].program, "grep");
        assert_eq!(segs[1].args, vec!["foo"]);
    }

    #[test]
    fn args_env_var_excluded() {
        let segs = parse_segments("ENV=val git status");
        assert_eq!(segs[0].program, "git");
        assert_eq!(segs[0].args, vec!["status"]);
    }

    #[test]
    fn args_redirection_excluded() {
        let segs = parse_segments("git log > file");
        assert_eq!(segs[0].program, "git");
        assert_eq!(segs[0].args, vec!["log"]);
    }

    #[test]
    fn args_double_dash_stops_flag_expansion() {
        let segs = parse_segments("rm -- -rf /tmp");
        assert_eq!(segs[0].program, "rm");
        assert_eq!(segs[0].args, vec!["--", "-rf", "/tmp"]);
    }

    // --- Wrapper arg forwarding ---

    #[test]
    fn wrapper_command_forwards_args() {
        let segs = parse_segments("command rm -rf /");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].program, "rm");
        assert_eq!(segs[0].args, vec!["-r", "-f", "/"]);
    }

    #[test]
    fn wrapper_env_forwards_args() {
        let segs = parse_segments("env rm -rf /");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].program, "rm");
        assert_eq!(segs[0].args, vec!["-r", "-f", "/"]);
    }

    #[test]
    fn wrapper_nohup_forwards_args() {
        let segs = parse_segments("nohup git push --force");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].program, "git");
        assert_eq!(segs[0].args, vec!["push", "--force"]);
    }

    #[test]
    fn wrapper_double_dash_stops_flag_expansion() {
        let segs = parse_segments("command rm -- -rf /tmp");
        assert_eq!(segs[0].program, "rm");
        assert_eq!(segs[0].args, vec!["--", "-rf", "/tmp"]);
    }

    // --- Flag expansion ---

    #[test]
    fn expand_flags_combined_short_flags() {
        assert_eq!(expand_flags("-rf"), vec!["-r", "-f"]);
    }

    #[test]
    fn expand_flags_long_flag_unchanged() {
        assert_eq!(expand_flags("--force"), vec!["--force"]);
    }

    #[test]
    fn expand_flags_single_short_flag_unchanged() {
        assert_eq!(expand_flags("-v"), vec!["-v"]);
    }

    #[test]
    fn expand_flags_positional_unchanged() {
        assert_eq!(expand_flags("filename"), vec!["filename"]);
    }

    #[test]
    fn expand_flags_bare_dash_unchanged() {
        assert_eq!(expand_flags("-"), vec!["-"]);
    }

    #[test]
    fn expand_flags_double_dash_unchanged() {
        assert_eq!(expand_flags("--"), vec!["--"]);
    }

    #[test]
    fn expand_flags_with_equals_unchanged() {
        assert_eq!(expand_flags("-rf=value"), vec!["-rf=value"]);
    }

    #[test]
    fn expand_flags_three_chars() {
        assert_eq!(expand_flags("-rvf"), vec!["-r", "-v", "-f"]);
    }
}
