use leafc_coreapi::diagnostic::{DiagTextColor, DiagnosticianApi};
use leafc_coreapi::lexer::LexerApi;
use leafc_coreapi::name_pass::{NamePassApi, NamePassError};
use leafc_coreapi::parser::{ParserApi, ParserResult};
use leafc_coreapi::source::SourcePool;
use leafc_coreapi::tokens_pass::TokenPassApi;
use leafc_lexer::Lexer;
use leafc_parser::Parser;
use leafc_diag::Diagnostician;
use leafc_tokenpass::Preprocessor;
use leafc_namepass::NamePass;

fn main() {
    let code = r#"
preprocessor_define ZERO 0
preprocessor_define MakeListVar(...) let name = #[__PreprocessorRestArgs]
fun main()
    MakeListVar(ZERO)
"#;
    let source_pool = SourcePool::new();
    let mut diag = Diagnostician::new(source_pool, DiagTextColor {
        diag_title: "\x1b[31m",       // 红
        diag_message: "\x1b[34m",     // 蓝
        diag_bar: "\x1b[35m",         // 紫
        diag_source_name: "\x1b[35m", // 紫
        diag_reset: "\x1b[0m",        // 重置
    });
    let source_id = diag.add_source("<TEST>".to_string(), code.to_string());

    let mut lex = Lexer::new(source_id, code.to_string());

    let tokens = match lex.tokenize() {
        Ok(tokens) => {
            for token in &tokens.data {
                println!("{:?}", token);
            }
            println!("== docstrings ==");
            for docstring in &lex.get_document_strings().data {
                println!("{:?} {:?}", docstring.data, docstring.span);
            }
            println!("== === ==");
            tokens
        },
        Err(e) => {
            println!("{}", diag.report(e));
            return;
        }
    };

    let new_tokens = match Preprocessor::new(&tokens, source_id)
        .pre_definitions(
            vec![
                    if cfg!(target_os = "windows") {
                        "__Windows".to_string()
                    } else if cfg!(target_os = "macos") {
                        "__Mac".to_string()
                    } else if cfg!(target_os = "linux") {
                        "__Linux".to_string()
                    } else {
                        "__Unknown".to_string()
                    }
                ]
        )
        .pass() {
        Ok(new_tokens) => {
            println!("\n\n== token pass ==\n\n");
    
            for token in &new_tokens.data {
                println!("{:?}", token);
            }
            println!("== === ==");
            return;
        }
        Err(e) => {
            print!("{}", diag.report(e));
            return;
        }
    };

    let mut file_ast = match Parser::new(source_id, &tokens).parse() {
        Ok(ParserResult {ast, requires})  => {
            println!("=== ast ===");

            println!("{:#?}", ast);
            println!("=== === ===");

            println!("=== requires ==");
            println!("{:?}", requires);
            println!("=== === ===");
            ast
        },
        Err(e) => {
            println!("{}", diag.report(e));
            return;
        }
    };

    let name_passed_ast = match NamePass::new(&file_ast).pass() {
        Ok((top_scope, scopes))  => {
            println!("=== scope ===");
            println!("{:#?}", top_scope);
            println!("=== === ===");

            println!("=== scope ===");
            println!("{:#?}", scopes);
            println!("=== === ===");
        },
        Err(e) => {
            println!("{}", diag.report(e));
            return;
        }
    };
}

