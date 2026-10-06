//! `zyt.ui`: asking the user something.
//!
//! The script describes a window and blocks on one call. Nothing of the
//! toolkit is reachable from here: the description is [`Form`], and whoever
//! draws it answers with a value per field. A question waved away answers with
//! nothing, and a run that was cancelled while the window stood answers with
//! an error rather than with nothing — a script told the user gave up would
//! carry on, and the line is not its any more.

use super::{Context, external};
use crate::error::ScriptError;
use crate::form::{Choice, Field, FieldKind, Form, Value};
use mlua::{Lua, Table};
use std::sync::Arc;

/// Builds the table.
pub(crate) fn table(lua: &Lua, context: &Arc<Context>) -> mlua::Result<Table> {
    let ui = lua.create_table()?;

    ui.set(
        "ask",
        lua.create_function({
            let context = context.clone();
            move |lua, described: Table| {
                context.check()?;
                let form = form_of(&described)?;
                let Some(answers) = context.prompt.ask(form.clone()) else {
                    context.check()?;
                    return Ok(mlua::Value::Nil);
                };
                form.check(&answers).map_err(external)?;

                let table = lua.create_table()?;
                for (name, value) in answers {
                    table.set(name, value_to_lua(lua, &value)?)?;
                }
                Ok(mlua::Value::Table(table))
            }
        })?,
    )?;

    ui.set(
        "confirm",
        lua.create_function({
            let context = context.clone();
            move |_, (question, detail): (String, Option<String>)| {
                context.check()?;
                let mut form = Form::new(&question);
                form.hint = detail;
                let answered = context.prompt.ask(form).is_some();
                if !answered {
                    context.check()?;
                }
                Ok(answered)
            }
        })?,
    )?;

    Ok(ui)
}

/// Reads the window a script described.
fn form_of(described: &Table) -> mlua::Result<Form> {
    let mut form = Form {
        id: described.get("id").unwrap_or_default(),
        unsaved: described.get("unsaved").unwrap_or(false),
        title: described.get("title").unwrap_or_default(),
        hint: described.get("hint").ok(),
        fields: Vec::new(),
        accept: described.get("accept").ok(),
        reject: described.get("reject").ok(),
    };

    let Ok(fields) = described.get::<Table>("fields") else {
        return Ok(form);
    };
    for (at, row) in fields.sequence_values::<Table>().enumerate() {
        form.fields.push(field_of(at, &row?)?);
    }
    Ok(form)
}

/// Reads one row of the window.
fn field_of(at: usize, row: &Table) -> mlua::Result<Field> {
    let kind: String = row.get("kind").unwrap_or_else(|_| "text".to_string());
    let kind = kind_of(&kind, row)?;
    let name: String = row
        .get("name")
        .unwrap_or_else(|_| format!("{}-{at}", kind.name()));

    Ok(Field {
        label: row.get("label").unwrap_or_else(|_| name.clone()),
        name,
        hint: row.get("hint").ok(),
        required: row.get("required").unwrap_or(false),
        kind,
    })
}

/// Reads what a row holds.
fn kind_of(kind: &str, row: &Table) -> mlua::Result<FieldKind> {
    match kind {
        "text" => Ok(FieldKind::Text {
            value: row.get("value").unwrap_or_default(),
            password: row.get("password").unwrap_or(false),
        }),
        "textarea" => Ok(FieldKind::Textarea {
            value: row.get("value").unwrap_or_default(),
            rows: row.get::<usize>("rows").unwrap_or(1).max(1),
        }),
        "switch" => Ok(FieldKind::Switch {
            value: row.get("value").unwrap_or(false),
        }),
        "one_of" => Ok(FieldKind::OneOf {
            options: options_of(row)?,
            value: row.get("value").unwrap_or_default(),
        }),
        "select" => Ok(FieldKind::Select {
            options: options_of(row)?,
            value: row.get("value").ok(),
        }),
        "many_of" => Ok(FieldKind::ManyOf {
            options: options_of(row)?,
            value: row.get("value").unwrap_or_default(),
        }),
        "note" => Ok(FieldKind::Note),
        "separator" => Ok(FieldKind::Separator),
        other => Err(external(ScriptError::Manifest {
            name: String::from("a dialog"),
            what: format!("{other} is no kind of field"),
        })),
    }
}

/// Reads what a row offers to pick from.
///
/// An entry is a word, a pair of words, or a table saying which is which, so a
/// list of plain names costs a script nothing to write and a list whose labels
/// are translated is still the same call.
fn options_of(row: &Table) -> mlua::Result<Vec<Choice>> {
    let Ok(listed) = row.get::<Table>("options") else {
        return Ok(Vec::new());
    };

    let mut options = Vec::new();
    for value in listed.sequence_values::<mlua::Value>() {
        match value? {
            mlua::Value::String(name) => options.push(Choice::new(&name.to_string_lossy())),
            mlua::Value::Table(entry) => options.push(choice_of(&entry)?),
            other => {
                return Err(external(ScriptError::Manifest {
                    name: String::from("a dialog"),
                    what: format!("{} is no entry of a list", other.type_name()),
                }));
            }
        }
    }
    Ok(options)
}

/// Reads one entry of a list, written either way round.
fn choice_of(entry: &Table) -> mlua::Result<Choice> {
    if let Ok(id) = entry.get::<String>("id") {
        return Ok(Choice {
            label: entry.get("label").unwrap_or_else(|_| id.clone()),
            id,
            hint: entry.get("hint").ok(),
        });
    }

    let id: String = entry.get(1).unwrap_or_default();
    let label: String = entry.get(2).unwrap_or_else(|_| id.clone());
    Ok(Choice {
        id,
        label,
        hint: entry.get(3).ok(),
    })
}

/// Hands one answer back to the script.
fn value_to_lua(lua: &Lua, value: &Value) -> mlua::Result<mlua::Value> {
    Ok(match value {
        Value::Flag(flag) => mlua::Value::Boolean(*flag),
        Value::Text(text) | Value::One(text) => mlua::Value::String(lua.create_string(text)?),
        Value::Many(names) => mlua::Value::Table(lua.create_sequence_from(names.clone())?),
    })
}
