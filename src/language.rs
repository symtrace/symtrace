use std::path::Path;

use crate::types::SupportedLanguage;

/// Detect the programming language from a file extension.
pub fn detect_language(file_path: &str) -> Option<SupportedLanguage> {
    let ext = Path::new(file_path).extension()?.to_str()?;
    match ext {
        "rs" => Some(SupportedLanguage::Rust),
        "js" | "jsx" | "mjs" | "cjs" => Some(SupportedLanguage::JavaScript),
        "ts" | "tsx" => Some(SupportedLanguage::TypeScript),
        "py" | "pyi" => Some(SupportedLanguage::Python),
        "java" => Some(SupportedLanguage::Java),
        "c" | "h" => Some(SupportedLanguage::C),
        "cpp" | "hpp" | "cc" | "cxx" | "h++" => Some(SupportedLanguage::Cpp),
        "go" => Some(SupportedLanguage::Go),
        "json" | "jsonc" => Some(SupportedLanguage::Json),
        _ => None,
    }
}

/// Get the tree-sitter Language for a given supported language.
pub fn get_tree_sitter_language(lang: SupportedLanguage) -> tree_sitter::Language {
    match lang {
        SupportedLanguage::Rust => tree_sitter_rust::LANGUAGE.into(),
        SupportedLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        SupportedLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        SupportedLanguage::Python => tree_sitter_python::LANGUAGE.into(),
        SupportedLanguage::Java => tree_sitter_java::LANGUAGE.into(),
        SupportedLanguage::C => tree_sitter_c::LANGUAGE.into(),
        SupportedLanguage::Cpp => tree_sitter_cpp::LANGUAGE.into(),
        SupportedLanguage::Go => tree_sitter_go::LANGUAGE.into(),
        SupportedLanguage::Json => tree_sitter_json::LANGUAGE.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SupportedLanguage;

    // ── detect_language ───────────────────────────────────────────────

    #[test]
    fn rust_extension() {
        assert_eq!(detect_language("foo.rs"), Some(SupportedLanguage::Rust));
    }

    #[test]
    fn js_extensions() {
        for ext in &["app.js", "mod.jsx", "bundle.mjs", "require.cjs"] {
            assert_eq!(
                detect_language(ext),
                Some(SupportedLanguage::JavaScript),
                "failed for {ext}"
            );
        }
    }

    #[test]
    fn ts_extensions() {
        assert_eq!(
            detect_language("index.ts"),
            Some(SupportedLanguage::TypeScript)
        );
        assert_eq!(
            detect_language("comp.tsx"),
            Some(SupportedLanguage::TypeScript)
        );
    }

    #[test]
    fn python_extensions() {
        assert_eq!(
            detect_language("script.py"),
            Some(SupportedLanguage::Python)
        );
        assert_eq!(detect_language("stub.pyi"), Some(SupportedLanguage::Python));
    }

    #[test]
    fn java_extension() {
        assert_eq!(detect_language("Main.java"), Some(SupportedLanguage::Java));
    }

    #[test]
    fn c_extensions() {
        assert_eq!(detect_language("main.c"), Some(SupportedLanguage::C));
        assert_eq!(detect_language("header.h"), Some(SupportedLanguage::C));
    }

    #[test]
    fn cpp_extensions() {
        for ext in &["main.cpp", "header.hpp", "mod.cc", "source.cxx", "head.h++"] {
            assert_eq!(
                detect_language(ext),
                Some(SupportedLanguage::Cpp),
                "failed for {ext}"
            );
        }
    }

    #[test]
    fn go_extension() {
        assert_eq!(detect_language("main.go"), Some(SupportedLanguage::Go));
    }

    #[test]
    fn json_extensions() {
        assert_eq!(
            detect_language("config.json"),
            Some(SupportedLanguage::Json)
        );
        assert_eq!(detect_language("data.jsonc"), Some(SupportedLanguage::Json));
    }

    #[test]
    fn unknown_extension_returns_none() {
        assert_eq!(detect_language("archive.zip"), None);
        assert_eq!(detect_language("Makefile"), None);
        assert_eq!(detect_language("README.md"), None);
        assert_eq!(detect_language("no_extension"), None);
    }

    #[test]
    fn path_with_directories_uses_last_extension() {
        assert_eq!(
            detect_language("src/utils/helpers.rs"),
            Some(SupportedLanguage::Rust)
        );
        assert_eq!(
            detect_language("com/example/Main.java"),
            Some(SupportedLanguage::Java)
        );
    }

    #[test]
    fn dotfile_without_extension_returns_none() {
        assert_eq!(detect_language(".gitignore"), None);
    }

    // ── get_tree_sitter_language ──────────────────────────────────────

    #[test]
    fn tree_sitter_language_roundtrip() {
        // Just verify it doesn't panic for each variant.
        for lang in [
            SupportedLanguage::Rust,
            SupportedLanguage::JavaScript,
            SupportedLanguage::TypeScript,
            SupportedLanguage::Python,
            SupportedLanguage::Java,
            SupportedLanguage::C,
            SupportedLanguage::Cpp,
            SupportedLanguage::Go,
            SupportedLanguage::Json,
        ] {
            let _ts_lang = get_tree_sitter_language(lang);
        }
    }
}
