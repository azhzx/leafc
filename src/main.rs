use leafc_coreapi::diagnostic::{Colors, DiagnosticianApi};
use leafc_coreapi::lexer::LexerApi;
use leafc_coreapi::source::SourcePool;
use leafc_coreapi::tokens_pass::TokenPassApi;
use leafc_lexer::Lexer;
use leafc_diag::Diagnostician;
use leafc_tokenpass::TokenPass;

fn main() {
    let code = r#"
__nightly__preprocess__ N 100
__nightly__preprocess__ M(v) let v = N

fun main()
    M(100)
"#;
    let source_pool = SourcePool::new();
    let mut diag = Diagnostician::new(source_pool, Colors {
        red: "\x1b[31m",
        green: "\x1b[32m",
        blue: "\x1b[34m",
        cyan: "\x1b[36m",
        pink: "\x1b[95m",
        purple: "\x1b[35m",
        reset_color: "\x1b[0m",
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
    
    // let new_token = match TokenPass::new(&tokens, source_id).pass() {
    //     Ok(token_pass) => {
    //         println!("\n\n== token pass ==\n\n");
    //
    //         for token in &token_pass.data {
    //             println!("{:?}", token);
    //         }
    //         println!("== === ==");
    //         token_pass
    //     }
    //     Err(e) => {
    //         print!("{}", diag.report(e));
    //         return;
    //     }
    // };
}

