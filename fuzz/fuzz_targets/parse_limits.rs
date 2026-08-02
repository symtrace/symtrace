#![no_main]
use libfuzzer-sys::fuzz_target;
use symtrace::ast_builder::parse_content;
use symtrace::types::{ParserLimits, SupportedLanguage};

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    let lang_choice = data[0] % 5;
    let lang = match lang_choice {
        0 => SupportedLanguage::Rust,
        1 => SupportedLanguage::JavaScript,
        2 => SupportedLanguage::Python,
        3 => SupportedLanguage::C,
        _ => SupportedLanguage::Json,
    };

    let text_slice = &data[1..];
    if let Ok(text) = std::str::from_utf8(text_slice) {
        let limits = ParserLimits {
            max_file_size_bytes: 50_000,
            max_ast_nodes: 5_000,
            max_recursion_depth: 128,
            parse_timeout_ms: 100,
        };

        let _ = parse_content(text, lang, false, &limits);
        let _ = parse_content(text, lang, true, &limits);
    }
});
