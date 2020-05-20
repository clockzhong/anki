use std::fs;
use std::path::Path;

use fluent_syntax::ast::{Entry::Message, ResourceEntry};
use fluent_syntax::parser::parse;
use std::collections::HashMap;

fn get_identifiers(ftl_text: &str) -> Vec<String> {
    let res = parse(ftl_text).unwrap();
    let mut idents = vec![];

    for entry in res.body {
        if let ResourceEntry::Entry(Message(m)) = entry {
            idents.push(m.id.name.to_string());
        }
    }

    idents.sort();

    idents
}

fn proto_enum(idents: &[String]) -> String {
    let mut buf = String::from(
        r#"// This file is automatically generated as part of the build process.

syntax = "proto3";
package backend_proto;
enum FluentString {
"#,
    );
    for (idx, s) in idents.iter().enumerate() {
        let name = s.replace("-", "_").to_uppercase();
        buf += &format!("  {} = {};\n", name, idx);
    }

    buf += "}\n";

    buf
}

fn rust_string_vec(idents: &[String]) -> String {
    let mut buf = String::from(
        r#"// This file is automatically generated as part of the build process.

pub(super) const FLUENT_KEYS: &[&str] = &[
"#,
    );

    for s in idents {
        buf += &format!("    \"{}\",\n", s);
    }

    buf += "];\n";

    buf
}

#[cfg(test)]
mod test {
    use crate::i18n::extract_idents::{get_identifiers, proto_enum, rust_string_vec};

    #[test]
    fn all() {
        let idents = get_identifiers("key-one = foo\nkey-two = bar");
        assert_eq!(idents, vec!["key-one", "key-two"]);

        assert_eq!(
            proto_enum(&idents),
            r#"// This file is automatically generated as part of the build process.

syntax = "proto3";
package backend_strings;
enum FluentString {
  KEY_ONE = 0;
  KEY_TWO = 1;
}
"#
        );

        assert_eq!(
            rust_string_vec(&idents),
            r#"// This file is automatically generated as part of the build process.

const FLUENT_KEYS: &[&str] = &[
    "key-one",
    "key-two",
];
"#
        );
    }
}

fn main() -> std::io::Result<()> {
    // write template.ftl
    let mut buf = String::new();
    let mut ftl_template_dirs = vec!["./ftl".to_string()];
    if let Ok(paths) = std::env::var("FTL_TEMPLATE_DIRS") {
        ftl_template_dirs.extend(paths.split(',').map(|s| s.to_string()));
    }
    for ftl_dir in ftl_template_dirs {
        let ftl_dir = Path::new(&ftl_dir);
        for entry in fs::read_dir(ftl_dir)? {
            let entry = entry?;
            let fname = entry.file_name().into_string().unwrap();
            if !fname.ends_with(".ftl") {
                continue;
            }
            let path = entry.path();
            println!("cargo:rerun-if-changed=./ftl/{}", fname);
            buf += &fs::read_to_string(path)?;
            buf.push('\n');
        }
    }
    let combined_ftl = Path::new("src/i18n/ftl/template.ftl");
    fs::write(combined_ftl, &buf)?;

    // generate code completion for ftl strings
    let idents = get_identifiers(&buf);
    let string_proto_path = Path::new("../proto/fluent.proto");
    fs::write(string_proto_path, proto_enum(&idents))?;
    let rust_string_path = Path::new("src/i18n/autogen.rs");
    fs::write(rust_string_path, rust_string_vec(&idents))?;

    // output protobuf generated code
    // we avoid default OUT_DIR for now, as it breaks code completion
    std::env::set_var("OUT_DIR", "src");
    println!("cargo:rerun-if-changed=../proto/backend.proto");
    prost_build::compile_protos(&["../proto/backend.proto"], &["../proto"]).unwrap();

    // write the other language ftl files
    let mut ftl_lang_dirs = vec!["./ftl/repo/core".to_string()];
    if let Ok(paths) = std::env::var("FTL_LOCALE_DIRS") {
        ftl_lang_dirs.extend(paths.split(',').map(|s| s.to_string()));
    }
    let mut langs = HashMap::new();
    for ftl_dir in ftl_lang_dirs {
        for ftl_dir in fs::read_dir(ftl_dir)? {
            let lang_dir = ftl_dir?;
            if lang_dir.file_name() == "templates" {
                continue;
            }
            let mut buf = String::new();
            let lang_name = lang_dir.file_name().into_string().unwrap();
            for entry in fs::read_dir(lang_dir.path())? {
                let entry = entry?;
                let path = entry.path();
                println!("cargo:rerun-if-changed={:?}", entry.path());
                buf += &fs::read_to_string(path)?;
                buf.push('\n');
            }
            langs
                .entry(lang_name)
                .or_insert_with(String::new)
                .push_str(&buf)
        }
    }

    for (lang, text) in langs {
        fs::write(format!("src/i18n/ftl/{}.ftl", lang), text)?;
    }

    Ok(())
}