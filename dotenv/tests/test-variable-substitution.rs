#![deny(clippy::uninlined_format_args)]

mod common;

use crate::common::tempdir_with_dotenv;
use std::{env, error};

#[test]
fn test_variable_substitutions() -> Result<(), Box<dyn error::Error>> {
    unsafe {
        env::set_var("KEY", "value");
        env::set_var("KEY1", "value1");
    }

    let substitutions_to_test = [
        "$ZZZ", "$KEY", "$KEY1", "${KEY}1", "$KEY_U", "${KEY_U}", "\\$KEY",
    ];

    let common_string = substitutions_to_test.join(">>");
    let txt = format!(
        r#"
KEY1=new_value1
KEY_U=$KEY+valueU

SUBSTITUTION_FOR_STRONG_QUOTES='{common_string}'
SUBSTITUTION_FOR_WEAK_QUOTES="{common_string}"
SUBSTITUTION_WITHOUT_QUOTES={common_string}
"#,
    );
    let dir = unsafe { tempdir_with_dotenv(&txt) }?;

    unsafe { dotenvy::dotenv() }?;

    assert_eq!(env::var("KEY")?, "value");
    assert_eq!(env::var("KEY1")?, "value1");
    assert_eq!(env::var("KEY_U")?, "value+valueU");
    assert_eq!(env::var("SUBSTITUTION_FOR_STRONG_QUOTES")?, common_string);
    assert_eq!(
        env::var("SUBSTITUTION_FOR_WEAK_QUOTES")?,
        [
            "",
            "value",
            "value1",
            "value1",
            "value_U",
            "value+valueU",
            "$KEY"
        ]
        .join(">>")
    );
    assert_eq!(
        env::var("SUBSTITUTION_WITHOUT_QUOTES")?,
        [
            "",
            "value",
            "value1",
            "value1",
            "value_U",
            "value+valueU",
            "$KEY"
        ]
        .join(">>")
    );

    env::set_current_dir(dir.path().parent().unwrap())?;
    dir.close()?;
    Ok(())
}
