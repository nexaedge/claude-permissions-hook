use brush_parser::ast;

/// A parsed segment of a shell command, representing one program invocation.
#[derive(Debug, PartialEq)]
pub struct CommandSegment {
    pub program: String,
}

/// Parse a shell command string into individual command segments.
///
/// Uses brush-parser to build a proper shell AST, then walks it to extract
/// program names. Correctly handles quoting, escaping, subshells, and
/// compound commands.
///
/// On parse errors, returns an empty Vec (fail-safe).
pub fn parse(command: &str) -> Vec<CommandSegment> {
    if command.trim().is_empty() {
        return vec![];
    }

    let mut parser = brush_parser::Parser::builder()
        .reader(std::io::Cursor::new(command.to_string()))
        .build();

    let program = match parser.parse_program() {
        Ok(program) => program,
        Err(_) => return vec![],
    };

    let mut segments = Vec::new();
    visit_program(&program, &mut segments);
    segments
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

fn visit_command(command: &ast::Command, segments: &mut Vec<CommandSegment>) {
    match command {
        ast::Command::Simple(simple) => {
            if let Some(word) = &simple.word_or_name {
                let name = word.flatten();
                if !name.is_empty() {
                    segments.push(CommandSegment { program: name });
                }
            }
        }
        ast::Command::Compound(compound, _) => visit_compound(compound, segments),
        ast::Command::Function(_) | ast::Command::ExtendedTest(_) => {}
    }
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
        ast::CompoundCommand::CaseClause(_) | ast::CompoundCommand::Arithmetic(_) => {}
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
        parse(input).into_iter().map(|s| s.program).collect()
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
        // Trailing && is invalid shell — parser returns error, fail-safe returns empty
        assert_eq!(programs("git add . &&"), Vec::<String>::new());
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
}
