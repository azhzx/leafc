#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use leafc_coreapi::lexer::{LexerApi, TokenStream};
    use leafc_coreapi::source::SourceId;
    use leafc_coreapi::token::TokenType;
    use crate::Lexer;
    use insta::Settings;


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

    #[test]
    fn test_simple_expression() {
        let src = "let x = 10 + 20 * 3";
        let empty_ops = HashMap::new();
        let mut lexer = Lexer::new(0, &src.to_string(), &empty_ops);
        let tokens = lexer.tokenize().unwrap();
        let snapshot = token_stream_snapshot(&tokens);

        insta::with_settings!({
        snapshot_path => crate_root().join("src").join("snapshots"),
        prepend_module_to_snapshot => false,
    }, {
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
        let empty_ops = HashMap::new();
        let mut lexer = Lexer::new(0, &src.to_string(), &empty_ops);
        let tokens = lexer.tokenize().unwrap();
        let snapshot = token_stream_snapshot(&tokens);

        insta::with_settings!({
        snapshot_path => crate_root().join("src").join("snapshots"),
        prepend_module_to_snapshot => false,
    }, {
        insta::assert_snapshot!(snapshot);
    });
    }

    #[test]
    fn test_empty_input() {
        let src = "";
        let empty_ops = HashMap::new();
        let mut lexer = Lexer::new(0, &src.to_string(), &empty_ops);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.data.len(), 1);
        assert_eq!(tokens.data[0].kind, TokenType::Eof);
    }

    #[test]
    fn test_identifiers_and_keywords() {
        let src = "hello world if else fun return";
        let empty_ops = HashMap::new();
        let mut lexer = Lexer::new(0, &src.to_string(), &empty_ops);
        let tokens = lexer.tokenize().unwrap();

        let kinds: Vec<_> = tokens.data.iter().map(|t| t.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                TokenType::Ident,
                TokenType::Ident,
                TokenType::KwIf,
                TokenType::KwElse,
                TokenType::KwFun,
                TokenType::KwReturn,
                TokenType::NewLine,
                TokenType::Eof,
            ]
        );
    }

    #[test]
    fn test_underscore_identifier() {
        let src = "_hidden _";
        let empty_ops = HashMap::new();
        let mut lexer = Lexer::new(0, &src.to_string(), &empty_ops);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.data[0].kind, TokenType::Ident);
        assert_eq!(tokens.data[0].text, "_hidden");
        assert_eq!(tokens.data[1].kind, TokenType::Ident);
        assert_eq!(tokens.data[1].text, "_");
    }

    #[test]
    fn test_integer_and_float() {
        let src = "123 45.67 .89";
        let empty_ops = HashMap::new();
        let mut lexer = Lexer::new(0, &src.to_string(), &empty_ops);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.data[0].kind, TokenType::Int);
        assert_eq!(tokens.data[0].text, "123");
        assert_eq!(tokens.data[1].kind, TokenType::Float);
        assert_eq!(tokens.data[1].text, "45.67");
        assert_eq!(tokens.data[2].kind, TokenType::Dot);
        assert_eq!(tokens.data[2].text, ".");
        assert_eq!(tokens.data[3].kind, TokenType::Int);
        assert_eq!(tokens.data[3].text, "89");
    }

    #[test]
    fn test_simple_string() {
        let src = r#""hello world""#;
        let empty_ops = HashMap::new();
        let mut lexer = Lexer::new(0, &src.to_string(), &empty_ops);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.data[0].kind, TokenType::String);
        assert_eq!(tokens.data[0].text, "hello world");
    }

    #[test]
    fn test_string_with_escape() {
        let src = r#""line1\nline2\t tab \"quote\\""#;
        let empty_ops = HashMap::new();
        let mut lexer = Lexer::new(0, &src.to_string(), &empty_ops);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.data[0].text, "line1\nline2\t tab \"quote\\");
    }

    #[test]
    fn test_unclosed_string_error() {
        let src = r#""unclosed"#;
        let empty_ops = HashMap::new();
        let mut lexer = Lexer::new(0, &src.to_string(), &empty_ops);
        let result = lexer.tokenize();
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_escape_sequence() {
        let src = r#""\k""#;
        let empty_ops = HashMap::new();
        let mut lexer = Lexer::new(0, &src.to_string(), &empty_ops);
        let result = lexer.tokenize();
        assert!(result.is_err());
    }

    #[test]
    fn test_line_comment() {
        let src = "abc // this is comment\n123";
        let empty_ops = HashMap::new();
        let mut lexer = Lexer::new(0, &src.to_string(), &empty_ops);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.data[0].kind, TokenType::Ident);
        assert_eq!(tokens.data[0].text, "abc");
        assert_eq!(tokens.data[1].kind, TokenType::NewLine);
        assert_eq!(tokens.data[2].kind, TokenType::Int);
        assert_eq!(tokens.data[2].text, "123");
    }

    #[test]
    fn test_doc_comment() {
        let src = "/// doc string\n123";
        let empty_ops = HashMap::new();
        let mut lexer = Lexer::new(0, &src.to_string(), &empty_ops);
        let tokens = lexer.tokenize().unwrap();
        let doc = lexer.get_document_strings();
        assert!(!doc.data.is_empty());
        assert_eq!(doc.data[0].data, " doc string");
    }

    #[test]
    fn test_builtin_operators() {
        let src = "+ - * / % = == != < > <= >= && || !";
        let empty_ops = HashMap::new();
        let mut lexer = Lexer::new(0, &src.to_string(), &empty_ops);
        let tokens = lexer.tokenize().unwrap();
        let expected = vec![
            TokenType::Plus, TokenType::Minus, TokenType::Star, TokenType::Slash,
            TokenType::Percent, TokenType::Eq, TokenType::EqEq, TokenType::Ne,
            TokenType::Lt, TokenType::Gt, TokenType::Le, TokenType::Ge,
            TokenType::And, TokenType::Or, TokenType::Not,
        ];
        let kinds: Vec<_> = tokens.data.iter().map(|t| t.kind.clone()).collect();
        assert_eq!(&kinds[..expected.len()], &expected[..]);
    }

    #[test]
    fn test_simple_indent() {
        let src = "fun main()\n    let x = 1";
        let empty_ops = HashMap::new();
        let mut lexer = Lexer::new(0, &src.to_string(), &empty_ops);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.data[0].kind, TokenType::KwFun);
        assert_eq!(tokens.data[4].kind, TokenType::NewLine);
        assert_eq!(tokens.data[5].kind, TokenType::Indent);
        assert_eq!(tokens.data[6].kind, TokenType::KwLet);
        let last_dedent = tokens.data.iter().rev().find(|t| t.kind == TokenType::Dedent);
        assert!(last_dedent.is_some());
    }

    #[test]
    fn test_indent_error() {
        let src = "fun main()\n   let x = 1";
        let empty_ops = HashMap::new();
        let mut lexer = Lexer::new(0, &src.to_string(), &empty_ops);
        let result = lexer.tokenize();
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_indent_levels() {
        let src = "a\n    b\n        c\n    d";
        let empty_ops = HashMap::new();
        let mut lexer = Lexer::new(0, &src.to_string(), &empty_ops);
        let tokens = lexer.tokenize().unwrap();
        let indent_dedent: Vec<_> = tokens.data.iter()
            .filter(|t| t.kind == TokenType::Indent || t.kind == TokenType::Dedent)
            .map(|t| t.kind.clone())
            .collect();
        assert_eq!(
            indent_dedent,
            vec![TokenType::Indent, TokenType::Indent, TokenType::Dedent, TokenType::Dedent]
        );
    }
}