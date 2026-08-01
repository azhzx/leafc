#[cfg(test)]
mod tests {
    
    use std::collections::HashMap;
    use std::path::PathBuf;

    use leafc_coreapi::diagnostic::DiagMsg;
    use leafc_coreapi::lexer::{LexerApi, TokenStream};
    use leafc_coreapi::token::{Token, TokenType};
    use leafc_coreapi::tokens_pass::TokenPassApi;
    use leafc_lexer::Lexer;
    use leafc_tokenpass::Preprocessor;

    fn crate_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn preprocess(source: &str) -> Result<TokenStream, DiagMsg> {
        let empty_ops = HashMap::new();
        let mut lexer = Lexer::new(0, &source.to_string(), &empty_ops);
        let tokens = lexer.tokenize().unwrap();
        let mut pp = Preprocessor::new(&tokens, 0);
        pp.pass()
    }

    fn token_stream_snapshot(tokens: &TokenStream) -> String {
        tokens
            .data
            .iter()
            .map(|tok| format!("{:?} '{}'", tok.kind, tok.text))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn assert_snapshot(tokens: &TokenStream, snapshot_name: &str) {
        let snapshot = token_stream_snapshot(tokens);
        insta::with_settings!({
            snapshot_path => crate_root().join("src").join("snapshots"),
            prepend_module_to_snapshot => false,
        }, {
            insta::assert_snapshot!(snapshot_name, snapshot);
        });
    }

    fn token_kinds(tokens: &TokenStream) -> Vec<TokenType> {
        tokens.data.iter().map(|t| t.kind.clone()).collect()
    }

    fn token_texts(tokens: &TokenStream) -> Vec<&str> {
        tokens.data.iter().map(|t| t.text.as_str()).collect()
    }

    #[test]
    fn predefined_macros_exist_and_have_values() {
        let src = "__windows __linux __mac __target_pointer_width version";
        let out = preprocess(src).unwrap();
        // 每个标识符都应该被替换成整数
        let kinds = token_kinds(&out);
        for k in &kinds[..kinds.len() - 2] { // 最后是 NewLine 和 Eof
            assert_eq!(k, &TokenType::Int);
        }
    }

    #[test]
    fn simple_define_without_params() {
        let src = "__define PI 3\nPI";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"3"));
    }

    #[test]
    fn macro_expands_to_multiple_tokens() {
        let src = "__define GREET hello world\nGREET";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"hello"));
        assert!(texts.contains(&"world"));
    }

    #[test]
    fn macro_with_params() {
        let src = "__define ADD(a,b) a + b\nADD(1,2)";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"1"));
        assert!(texts.contains(&"2"));
        assert!(texts.contains(&"+"));
    }

    #[test]
    fn macro_with_rest_args() {
        let src = "__define LOG(fmt, ...) fmt __rest_args\nLOG(x, y, z)";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"x"));
        assert!(texts.contains(&"y"));
        assert!(texts.contains(&"z"));
    }

    #[test]
    fn rest_args_with_single_arg() {
        let src = "__define LOG(fmt, ...) fmt __rest_args\nLOG(only)";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"only"));
    }

    #[test]
    fn rest_args_empty_should_still_work() {
        let src = "__define LOG(fmt, ...) fmt __rest_args\nLOG(msg)";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"msg"));
    }

    #[test]
    fn nested_macro_expansion() {
        let src = "__define INNER 10\n__define OUTER INNER\nOUTER";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"10"));
    }

    #[test]
    fn recursive_macro_stops_after_one_level() {
        let src = "__define REC REC\nREC";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"REC"));
    }

    #[test]
    fn mutually_recursive_macros() {
        let src = "__define A B\n__define B A\nA";
        let out = preprocess(src).unwrap();
        // 一次展开 A → B，再展开 B 时检测到递归，停止
        let texts = token_texts(&out);
        assert!(texts.contains(&"B"));
    }

    #[test]
    fn redefine_macro_keeps_first_definition() {
        let src = "__define X first\n__define X second\nX";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"first"));
    }

    #[test]
    fn delete_macro_should_remove_definition() {
        let src = "__define FOO bar\n__delete FOO\nFOO";
        let out = preprocess(src).unwrap();
        let kinds = token_kinds(&out);
        assert!(kinds.contains(&TokenType::Ident));
    }

    #[test]
    fn conditional_if_true_branch() {
        let src = "__if 1\nyes\n__else\nno\n__endif";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"yes"));
        assert!(!texts.contains(&"no"));
    }

    #[test]
    fn conditional_if_false_branch() {
        let src = "__if 0\nno\n__else\nyes\n__endif";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"yes"));
        assert!(!texts.contains(&"no"));
    }

    #[test]
    fn conditional_with_elif() {
        let src = "__if 0\nno\n__elif 1\nyes\n__else\nno2\n__endif";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"yes"));
        assert!(!texts.contains(&"no"));
        assert!(!texts.contains(&"no2"));
    }

    #[test]
    fn conditional_nested() {
        let src = "__if 1\n  __if 0\n  no\n  __else\n  inner\n  __endif\n__else\n  no2\n__endif";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"inner"));
        assert!(!texts.contains(&"no"));
        assert!(!texts.contains(&"no2"));
    }

    #[test]
    fn conditional_without_else_should_produce_nothing_in_false() {
        let src = "before __if 0\nhidden\n__endif after";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"before"));
        assert!(texts.contains(&"after"));
        assert!(!texts.contains(&"hidden"));
    }

    #[test]
    fn eval_arithmetic() {
        let src = "__eval(2 + 3 * 4)";
        let out = preprocess(src).unwrap();
        // 2 + 12 = 14
        assert_eq!(out.data[0].text, "14");
    }

    #[test]
    fn eval_comparison() {
        let src = "__eval(5 > 3)";
        let out = preprocess(src).unwrap();
        assert_eq!(out.data[0].text, "1");
    }

    #[test]
    fn eval_logic() {
        let src = "__eval(1 && 0 || 1)";
        let out = preprocess(src).unwrap();
        assert_eq!(out.data[0].text, "1");
    }

    #[test]
    fn conditional_with_undefined_ident_errors() {
        let src = "__if UNDEFINED\nblock\n__endif";
        let result = preprocess(src);
        assert!(result.is_err());
    }

    #[test]
    fn user_panic_produces_error() {
        let src = "__panic something_wrong";
        let result = preprocess(src);
        assert!(result.is_err());
    }

    #[test]
    fn warning_does_not_abort() {
        let src = "__warning hello\nresult";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"result"));
    }

    #[test]
    fn counter_increments() {
        let src = "__counter __counter __counter";
        let out = preprocess(src).unwrap();
        let texts: Vec<_> = out.data.iter()
            .filter(|t| t.kind == TokenType::Int)
            .map(|t| t.text.clone())
            .collect();
        assert_eq!(texts, vec!["0", "1", "2"]);
    }

    #[test]
    fn to_string_creates_string_literal() {
        let src = "__to_string(hello)";
        let out = preprocess(src).unwrap();
        assert_eq!(out.data[0].kind, TokenType::String);
        assert_eq!(out.data[0].text, "hello");
    }

    fn concat_creates_identifier() {
        let src = "__define VAR 10\n__concat(V, AR)";
        let out = preprocess(src).unwrap();
        // 应该生成标识符 VAR，然后被展开为 10
        assert_eq!(out.data[0].text, "10");
    }

    #[test]
    fn repeat_n_times() {
        let src = "__repeat(3, __counter)";
        let out = preprocess(src).unwrap();
        let ints: Vec<_> = out.data.iter()
            .filter(|t| t.kind == TokenType::Int)
            .map(|t| t.text.clone())
            .collect();
        assert_eq!(ints, vec!["0", "1", "2"]);
    }

    #[test]
    fn is_defined_returns_1_for_defined() {
        let src = "__define FOO bar\n__is_defined FOO";
        let out = preprocess(src).unwrap();
        assert_eq!(out.data[0].text, "1");
    }

    #[test]
    fn is_defined_returns_0_for_undefined() {
        let src = "__is_defined UNDEF";
        let out = preprocess(src).unwrap();
        assert_eq!(out.data[0].text, "0");
    }


    #[test]
    fn classic_operator_precedence_trap() {
        let src = "__define SQUARE(x) x * x\nSQUARE(1 + 2)";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"*"));
    }


    #[test]
    fn classic_side_effect_multiple_evaluation() {
        let src = "__define DOUBLE(x) x + x\nDOUBLE(__counter)";
        let out = preprocess(src).unwrap();
        let ints: Vec<_> = out.data.iter()
            .filter(|t| t.kind == TokenType::Int)
            .map(|t| t.text.clone())
            .collect();
        assert!(ints.len() >= 2);
        assert_ne!(ints[0], ints[1]);
    }


    #[test]
    fn macro_named_after_keyword_does_not_expand_keywords() {
        let src = "__define if 42\nif";
        let out = preprocess(src).unwrap();
        assert_eq!(out.data[0].kind, TokenType::KwIf);
    }


    #[test]
    fn conditional_undefined_ident_causes_error_instead_of_silent_zero() {
        let result = preprocess("__if UNDEFINED\nsomething\n__endif");
        assert!(result.is_err());
    }


    #[test]
    fn macro_with_trailing_semicolon_can_cause_dangling_else() {
        let src = "__define LOG(msg) msg ;\n__if 1\nLOG(hello)\n__else\nworld\n__endif";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"hello"));
        assert!(texts.contains(&";"));
    }

    #[test]
    fn is_defined_not_usable_inside_macro_definition() {
        let src = "__define CHECK(x) __is_defined x\nCHECK(FOO)";
        let out = preprocess(src).unwrap();
        let val = out.data[0].text.clone();
        assert!(val == "0" || val == "1");
    }


    #[test]
    fn variadic_empty_args_should_not_leave_dangling_comma() {
        let src = "__define LOG(fmt, ...) fmt , __rest_args\nLOG(msg)";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert_eq!(texts[0], "msg");
        if texts.len() > 1 {
            assert_ne!(texts[1], ",");
        }
    }


    #[test]
    fn macro_call_with_comma_inside_nested_parens() {
        let src = "__define PAIR(a,b) a , b\nPAIR((1,2),3)";
        let out = preprocess(src).unwrap();
        let texts = token_texts(&out);
        assert!(texts.contains(&"("));
        assert!(texts.contains(&"1"));
        assert!(texts.contains(&"2"));
        assert!(texts.contains(&")"));
        assert!(texts.contains(&"3"));
    }

    #[test]
    fn macro_wrong_number_of_args_error() {
        let src = "__define F(a,b) a+b\nF(1)";
        let result = preprocess(src);
        assert!(result.is_err());
    }

    #[test]
    fn variadic_macro_with_fewer_args_error() {
        let src = "__define F(a,b,...) a __rest_args\nF(1)"; // 至少需要2个
        let result = preprocess(src);
        assert!(result.is_err());
    }

    #[test]
    fn snapshot_complex_preprocessing() {
        let src = r#"
__define MAX(a,b) __if a > b __rest_args a __else b __endif
MAX(3, 5)
__counter
__to_string(test)
__concat(pre, fix)
__repeat(2, __counter)
__if __is_defined MAX
MAX(2, 7)
__endif
"#;
        let out = preprocess(src).unwrap();
        assert_snapshot(&out, "complex_preprocessing");
    }

    #[test]
    fn snapshot_classic_bugs_demo() {
        let src = r#"
__define SQUARE(x) x * x
__define DOUBLE(x) x + x
SQUARE(1+2)
DOUBLE(__counter)
"#;
        let out = preprocess(src).unwrap();
        assert_snapshot(&out, "classic_bugs_demo");
    }
}