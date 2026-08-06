#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use leafc_coreapi::crate_meta::{BuiltinOperator, OperatorDef, OperatorKind};
    use leafc_coreapi::error_items::{
        DiagColorConfig, DiagCtx, DEFAULT_EN_TOML, SourceMap, TomlLocalizer,
    };
    use leafc_coreapi::lexer::{LexerApi, TokenStream};
    use leafc_coreapi::source::{SourceId, SourcePool};
    use leafc_coreapi::token::TokenType;
    use crate::Lexer;
    use insta::Settings;

    /// helpers

    fn crate_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn snapshot_flat_config() -> Settings {
        let mut cfg = Settings::new();
        let snapshot_dir = crate_root().join("src").join("snapshots");
        cfg.set_snapshot_path(snapshot_dir);
        cfg.set_prepend_module_to_snapshot(false);
        cfg
    }

    fn token_stream_snapshot(tokens: &TokenStream) -> String {
        tokens
            .data
            .iter()
            .map(|tok| format!("{:?} '{}'", tok.kind, tok.text))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn new_diag(src: &str) -> (DiagCtx, SourceId) {
        let mut pool = SourcePool(Vec::new());
        let source_id = pool.add_source("<test>".to_string(), src.to_string());
        let source_map = SourceMap::new(pool);
        let localizer = TomlLocalizer::new(DEFAULT_EN_TOML, DEFAULT_EN_TOML).expect("localizer");
        let diag = DiagCtx::new(source_map, Box::new(localizer), DiagColorConfig::default());
        (diag, source_id)
    }

    fn tokenize(src: &str) -> (TokenStream, DiagCtx) {
        tokenize_with_ops(src, &HashMap::new())
    }

    fn tokenize_with_ops(src: &str, ops: &HashMap<String, OperatorDef>) -> (TokenStream, DiagCtx) {
        let (mut diag, source_id) = new_diag(src);
        let mut lexer = Lexer::new(source_id, src, ops);
        let tokens = lexer.tokenize(&mut diag);
        (tokens, diag)
    }

    fn kinds(tokens: &TokenStream) -> Vec<TokenType> {
        tokens.data.iter().map(|t| t.kind.clone()).collect()
    }

    fn texts(tokens: &TokenStream) -> Vec<&str> {
        tokens.data.iter().map(|t| t.text.as_str()).collect()
    }

    fn assert_no_errors(diag: &DiagCtx) {
        assert!(!diag.has_errors(), "unexpected diagnostics:\n{}", diag.emit_all());
    }

    fn assert_error(diag: &DiagCtx) {
        assert!(diag.has_errors(), "expected a diagnostic but none was emitted");
    }

    /// keywords

    const KEYWORDS: &[(&str, TokenType)] = &[
        ("is", TokenType::KwIs),
        ("typeof", TokenType::KwTypeOf),
        ("use", TokenType::KwUse),
        ("of", TokenType::KwOf),
        ("ref", TokenType::KwRef),
        ("or", TokenType::KwOr),
        ("and", TokenType::KwAnd),
        ("not", TokenType::KwNot),
        ("as", TokenType::KwAs),
        ("fun", TokenType::KwFun),
        ("return", TokenType::KwReturn),
        ("symdef", TokenType::KwSymDef),
        ("symexpr", TokenType::KwSymExpr),
        ("abst", TokenType::KwAbst),
        ("mut", TokenType::KwMut),
        ("with", TokenType::KwWith),
        ("let", TokenType::KwLet),
        ("const", TokenType::KwConst),
        ("bindto", TokenType::KwBindTo),
        ("binding", TokenType::KwBinding),
        ("move", TokenType::KwMove),
        ("copy", TokenType::KwCopy),
        ("do", TokenType::KwDo),
        ("it", TokenType::KwIt),
        ("global", TokenType::KwGlobal),
        ("share", TokenType::KwShare),
        ("if", TokenType::KwIf),
        ("then", TokenType::KwThen),
        ("else", TokenType::KwElse),
        ("elif", TokenType::KwElif),
        ("when", TokenType::KwWhen),
        ("guard", TokenType::KwGuard),
        ("handle", TokenType::KwHandle),
        ("effect", TokenType::KwEffect),
        ("catch", TokenType::KwCatch),
        ("resume", TokenType::KwResume),
        ("raise", TokenType::KwRaise),
        ("external", TokenType::KwExternal),
        ("ctype", TokenType::KwCType),
        ("pub", TokenType::KwPub),
        ("unsafe_call_external", TokenType::KwUnsafeCallExternal),
        ("type", TokenType::KwType),
        ("where", TokenType::KwWhere),
        ("no", TokenType::KwNo),
        ("only", TokenType::KwOnly),
        ("impl", TokenType::KwImpl),
        ("for", TokenType::KwFor),
        ("subtype", TokenType::KwSubType),
        ("basetype", TokenType::KwBaseType),
    ];

    #[test]
    fn all_keywords_are_lexed_as_keywords() {
        for (word, expected) in KEYWORDS {
            let (tokens, diag) = tokenize(word);
            assert_no_errors(&diag);
            assert_eq!(&tokens.data[0].kind, expected, "keyword `{}`", word);
            assert_eq!(&tokens.data[0].text, word);
        }
    }

    #[test]
    fn keywords_are_not_part_of_longer_identifiers() {
        let (tokens, diag) = tokenize("function fun ftypeof typeof");
        assert_no_errors(&diag);
        let ks = kinds(&tokens);
        assert_eq!(ks[0], TokenType::Ident); // function
        assert_eq!(ks[1], TokenType::KwFun); // fun
        assert_eq!(ks[2], TokenType::Ident); // ftypeof
        assert_eq!(ks[3], TokenType::KwTypeOf); // typeof
    }

    /// identifiers

    #[test]
    fn identifiers_and_underscore() {
        let (tokens, diag) = tokenize("_hidden _ foo123");
        assert_no_errors(&diag);
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenType::Ident,
                TokenType::Ident,
                TokenType::Ident,
                TokenType::NewLine,
                TokenType::Eof,
            ]
        );
        assert_eq!(texts(&tokens)[..3], ["_hidden", "_", "foo123"]);
    }

    #[test]
    fn unicode_identifiers() {
        let (tokens, diag) = tokenize("变量 常量 αβγ");
        assert_no_errors(&diag);
        let ks = kinds(&tokens);
        assert_eq!(ks[0], TokenType::Ident);
        assert_eq!(ks[1], TokenType::Ident);
        assert_eq!(ks[2], TokenType::Ident);
    }

    /// numbers

    #[test]
    fn integers_and_floats() {
        let (tokens, diag) = tokenize("123 0 45.67 0.5");
        assert_no_errors(&diag);
        let ks = kinds(&tokens);
        assert_eq!(ks[0], TokenType::Int);
        assert_eq!(ks[1], TokenType::Int);
        assert_eq!(ks[2], TokenType::Float);
        assert_eq!(ks[3], TokenType::Float);
        assert_eq!(texts(&tokens)[..4], ["123", "0", "45.67", "0.5"]);
    }

    #[test]
    fn dot_after_number_binds_to_the_number() {
        let (tokens, diag) = tokenize("123. .89");
        assert_no_errors(&diag);
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenType::Float, // 123.
                TokenType::Dot, // .
                TokenType::Int, // 89
                TokenType::NewLine,
                TokenType::Eof,
            ]
        );
    }

    /// strings

    #[test]
    fn string_literals() {
        let (tokens, diag) = tokenize(r#""" "hello world""#);
        assert_no_errors(&diag);
        assert_eq!(kinds(&tokens)[0], TokenType::String);
        assert_eq!(texts(&tokens)[0], "");
        assert_eq!(kinds(&tokens)[1], TokenType::String);
        assert_eq!(texts(&tokens)[1], "hello world");
    }

    #[test]
    fn string_escape_sequences() {
        let (tokens, diag) = tokenize(r#""\n\t\r\\\"\0""#);
        assert_no_errors(&diag);
        assert_eq!(texts(&tokens)[0], "\n\t\r\\\"\0");
    }

    #[test]
    fn invalid_escape_reports_error() {
        let (_tokens, diag) = tokenize(r#""\k""#);
        assert_error(&diag);
    }

    #[test]
    fn unclosed_string_reports_error_and_recovers() {
        let (tokens, diag) = tokenize(r#""unclosed"#);
        assert_error(&diag);
        // 报错后仍应产出完整 token 流（以 Eof 收尾）
        assert_eq!(tokens.data.last().map(|t| t.kind.clone()), Some(TokenType::Eof));
    }

    /// comments

    #[test]
    fn line_comment_is_skipped() {
        let (tokens, diag) = tokenize("abc // this is a comment\n123");
        assert_no_errors(&diag);
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenType::Ident,
                TokenType::NewLine,
                TokenType::Int,
                TokenType::NewLine,
                TokenType::Eof,
            ]
        );
    }

    #[test]
    fn doc_comment_is_collected() {
        let src = "/// doc string\n123";
        let (mut diag, source_id) = new_diag(src);
        let mut lexer = Lexer::new(source_id, src, &HashMap::new());
        let _tokens = lexer.tokenize(&mut diag);
        assert_no_errors(&diag);
        let docs = lexer.get_document_strings();
        assert_eq!(docs.data.len(), 1);
        assert_eq!(docs.data[0].data, " doc string");
    }

    #[test]
    fn blank_and_comment_only_lines_do_not_affect_indentation() {
        let (tokens, diag) = tokenize("a\n\n// note\nb");
        assert_no_errors(&diag);
        let ks = kinds(&tokens);
        assert!(!ks.contains(&TokenType::Indent));
        assert!(!ks.contains(&TokenType::Dedent));
    }

    /// operators

    #[test]
    fn single_char_operators() {
        let src = "+ - * / % & | ^ ! = < > ( ) { } [ ] , : ; # @ .";
        let (tokens, diag) = tokenize(src);
        assert_no_errors(&diag);
        let expected = vec![
            TokenType::Plus, TokenType::Minus, TokenType::Star, TokenType::Slash,
            TokenType::Percent, TokenType::Amp, TokenType::Pipe, TokenType::Caret,
            TokenType::Not, TokenType::Eq, TokenType::Lt, TokenType::Gt,
            TokenType::Lparen, TokenType::Rparen, TokenType::Lbrace, TokenType::Rbrace,
            TokenType::Lbracket, TokenType::Rbracket, TokenType::Comma, TokenType::Colon,
            TokenType::Semicolon, TokenType::Hash, TokenType::At, TokenType::Dot,
        ];
        let ks = kinds(&tokens);
        assert_eq!(&ks[..expected.len()], &expected[..]);
    }

    #[test]
    fn multi_char_operators() {
        let src = "== != <= >= && || << >> += -= *= /= %= &= |= ^= <<= >>= -> => .. ... |>";
        let (tokens, diag) = tokenize(src);
        assert_no_errors(&diag);
        let expected = vec![
            TokenType::EqEq, TokenType::Ne, TokenType::Le, TokenType::Ge,
            TokenType::And, TokenType::Or, TokenType::Shl, TokenType::Shr,
            TokenType::PlusEq, TokenType::MinusEq, TokenType::StarEq, TokenType::SlashEq,
            TokenType::PercentEq, TokenType::AmpEq, TokenType::PipeEq, TokenType::CaretEq,
            TokenType::ShlEq, TokenType::ShrEq,
            TokenType::Arrow, TokenType::FatArrow, TokenType::DotDot, TokenType::DotDotDot,
            TokenType::PipeLine,
        ];
        let ks = kinds(&tokens);
        assert_eq!(&ks[..expected.len()], &expected[..]);
    }

    #[test]
    fn longest_match_wins() {
        let (tokens, diag) = tokenize("=== <==");
        assert_no_errors(&diag);
        let ks = kinds(&tokens);
        assert_eq!(
            &ks[..4],
            &[TokenType::EqEq, TokenType::Eq, TokenType::Le, TokenType::Eq]
        );
    }

    #[test]
    fn user_defined_operators_are_lexed() {
        let mut ops = HashMap::new();
        ops.insert(
            "at".to_string(),
            OperatorDef {
                text: "@".to_string(),
                is_pub_external: false,
                high_than: Some(BuiltinOperator::Mul),
                less_than: None,
                kind: OperatorKind::Postfix,
            },
        );
        ops.insert(
            "spaceship".to_string(),
            OperatorDef {
                text: "<>".to_string(),
                is_pub_external: false,
                high_than: None,
                less_than: Some(BuiltinOperator::Mul),
                kind: OperatorKind::Infix,
            },
        );
        let (tokens, diag) = tokenize_with_ops("a @ b <> c", &ops);
        assert_no_errors(&diag);
        let ks = kinds(&tokens);
        assert_eq!(
            &ks[..5],
            &[
                TokenType::Ident,
                TokenType::UserOp,
                TokenType::Ident,
                TokenType::UserOp,
                TokenType::Ident,
            ]
        );
    }

    #[test]
    fn unknown_symbols_report_errors() {
        let (tokens, diag) = tokenize("~ $ ?");
        assert_error(&diag);
        assert_eq!(diag.collector.errors.len(), 3);
        assert_eq!(tokens.data.last().map(|t| t.kind.clone()), Some(TokenType::Eof));
    }

    /// indentation

    #[test]
    fn simple_indent_and_dedent() {
        let (tokens, diag) = tokenize("a\n    b");
        assert_no_errors(&diag);
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenType::Ident, // a
                TokenType::NewLine,
                TokenType::Indent, // 4 spaces
                TokenType::Ident, // b
                TokenType::NewLine,
                TokenType::Dedent, // at EOF
                TokenType::Eof,
            ]
        );
    }

    #[test]
    fn multiple_indent_levels() {
        let (tokens, diag) = tokenize("a\n    b\n        c\n    d");
        assert_no_errors(&diag);
        let indent_dedent: Vec<_> = tokens
            .data
            .iter()
            .filter(|t| matches!(t.kind, TokenType::Indent | TokenType::Dedent))
            .map(|t| t.kind.clone())
            .collect();
        assert_eq!(
            indent_dedent,
            vec![
                TokenType::Indent,
                TokenType::Indent,
                TokenType::Dedent,
                TokenType::Dedent,
            ]
        );
    }

    #[test]
    fn tabs_count_as_four_spaces() {
        let (tokens, diag) = tokenize("a\n\tb");
        assert_no_errors(&diag);
        assert_eq!(kinds(&tokens)[2], TokenType::Indent);
        assert_eq!(tokens.data[2].text, "    ");
    }

    #[test]
    fn invalid_indent_reports_error() {
        let (tokens, diag) = tokenize("a\n   b");
        assert_error(&diag);
        assert_eq!(tokens.data.last().map(|t| t.kind.clone()), Some(TokenType::Eof));
    }

    /// robustness

    #[test]
    fn adversarial_inputs_do_not_panic() {
        let inputs = [
            "",
            "\n",
            "\r\n",
            "\r\n\r\n",
            "   ",
            "\t",
            "\"",
            r#""\""#,
            "\\",
            "//",
            "///",
            "////",
            "1.",
            ".",
            "..",
            "...",
            "===",
            "a\n   b",
            "\"unterminated\nline",
            "a\n\t\n\tb",
            "0x1f",
            "1.2.3",
            "@",
            "#",
            "|",
            "||",
            "|>",
            "a\r\nb",
        ];
        for src in inputs {
            let (tokens, diag) = tokenize(src);
            assert_eq!(
                tokens.data.last().map(|t| t.kind.clone()),
                Some(TokenType::Eof),
                "input: {:?}",
                src
            );
            let _ = diag;
        }
    }

    #[test]
    fn trailing_newline_is_not_duplicated() {
        let (tokens, diag) = tokenize("a\n");
        assert_no_errors(&diag);
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenType::Ident,
                TokenType::NewLine,
                TokenType::Eof,
            ]
        );
    }

    /// snapshots

    #[test]
    fn simple_expression() {
        let (tokens, diag) = tokenize("let x = 10 + 20 * 3");
        assert_no_errors(&diag);
        let snapshot = token_stream_snapshot(&tokens);
        snapshot_flat_config().bind(|| {
            insta::assert_snapshot!(snapshot);
        });
    }

    #[test]
    fn snapshot_full_program() {
        let src = r#"
fun main()
    let x = 10
    let y = 20.5
    if x > y
        return x
    else
        return y
"#;
        let (tokens, diag) = tokenize(src);
        assert_no_errors(&diag);
        let snapshot = token_stream_snapshot(&tokens);
        snapshot_flat_config().bind(|| {
            insta::assert_snapshot!(snapshot);
        });
    }
}
