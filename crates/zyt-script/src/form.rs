//! A dialog as data: what is asked, and what comes back.
//!
//! A script does not draw. It describes what it wants to know and blocks on
//! one call; whoever is drawing builds the window from that description and
//! answers with a value per field. The same description is answered from the
//! command line, which is what makes a dialog testable with no window at all.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One entry of a list a field offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    /// What the answer carries when this entry is picked.
    pub id: String,
    /// What the entry is called.
    pub label: String,
    /// The sentence shown beside it, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl Choice {
    /// An entry whose identifier is also its label.
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            label: id.to_string(),
            hint: None,
        }
    }

    /// An entry called one thing and answering with another.
    pub fn named(id: &str, label: &str) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            hint: None,
        }
    }
}

/// What a field is, and what it starts out holding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldKind {
    /// One line of text.
    Text {
        /// What stands in it before anything is typed.
        #[serde(default)]
        value: String,
        /// True when what is typed is not shown.
        #[serde(default)]
        password: bool,
    },
    /// Text of several lines, starting at the height of one and growing.
    Textarea {
        /// What stands in it before anything is typed.
        #[serde(default)]
        value: String,
        /// How many lines tall it starts, at least one.
        #[serde(default = "one_row")]
        rows: usize,
    },
    /// One thing that is either on or off.
    Switch {
        /// Which way it starts.
        #[serde(default)]
        value: bool,
    },
    /// One of several, and never none of them.
    OneOf {
        /// What there is to pick from.
        options: Vec<Choice>,
        /// Which one starts picked; the first when the name is not there.
        #[serde(default)]
        value: String,
    },
    /// One of a list, picked from a menu.
    Select {
        /// What there is to pick from.
        options: Vec<Choice>,
        /// Which one starts picked, when one does.
        #[serde(default)]
        value: Option<String>,
    },
    /// Any number of a list, none of them included.
    ManyOf {
        /// What there is to pick from.
        options: Vec<Choice>,
        /// Which ones start picked.
        #[serde(default)]
        value: Vec<String>,
    },
    /// A line of text that asks nothing.
    Note,
    /// A line drawn between what is above and what is below.
    Separator,
}

/// One line of 1.
fn one_row() -> usize {
    1
}

/// True for a switch that is off, which is what is left out of a file.
fn is_not(flag: &bool) -> bool {
    !*flag
}

impl FieldKind {
    /// True when this kind asks the user for nothing.
    pub fn is_decoration(&self) -> bool {
        matches!(self, Self::Note | Self::Separator)
    }

    /// The word this kind is written under.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Text { .. } => "text",
            Self::Textarea { .. } => "textarea",
            Self::Switch { .. } => "switch",
            Self::OneOf { .. } => "one_of",
            Self::Select { .. } => "select",
            Self::ManyOf { .. } => "many_of",
            Self::Note => "note",
            Self::Separator => "separator",
        }
    }

    /// What this field answers with before anybody touches it.
    ///
    /// A field of one out of several answers with the first entry when it was
    /// given no value of its own: one of them is always picked, so there is no
    /// state in which it answers with nothing.
    pub fn default_value(&self) -> Option<Value> {
        match self {
            Self::Text { value, .. } | Self::Textarea { value, .. } => {
                Some(Value::Text(value.clone()))
            }
            Self::Switch { value } => Some(Value::Flag(*value)),
            Self::OneOf { options, value } => {
                let picked = options
                    .iter()
                    .find(|choice| choice.id == *value)
                    .or_else(|| options.first())?;
                Some(Value::One(picked.id.clone()))
            }
            Self::Select { options, value } => match value {
                Some(value) if options.iter().any(|choice| choice.id == *value) => {
                    Some(Value::One(value.clone()))
                }
                _ => None,
            },
            Self::ManyOf { options, value } => Some(Value::Many(
                value
                    .iter()
                    .filter(|name| options.iter().any(|choice| choice.id == **name))
                    .cloned()
                    .collect(),
            )),
            Self::Note | Self::Separator => None,
        }
    }

    /// What there is to pick from, for a kind that offers a list.
    pub fn options(&self) -> &[Choice] {
        match self {
            Self::OneOf { options, .. }
            | Self::Select { options, .. }
            | Self::ManyOf { options, .. } => options,
            _ => &[],
        }
    }
}

/// One row of a dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    /// Name the answer stands under.
    pub name: String,
    /// What the row is called.
    #[serde(default)]
    pub label: String,
    /// The sentence the pointer uncovers, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// True when the dialog cannot be accepted while this field is empty.
    #[serde(default)]
    pub required: bool,
    /// What the row holds.
    #[serde(flatten)]
    pub kind: FieldKind,
}

impl Field {
    /// A row of the given kind, named and labelled the same.
    pub fn new(name: &str, kind: FieldKind) -> Self {
        Self {
            name: name.to_string(),
            label: name.to_string(),
            hint: None,
            required: false,
            kind,
        }
    }

    /// The same row under another label.
    pub fn labelled(mut self, label: &str) -> Self {
        self.label = label.to_string();
        self
    }

    /// The same row with a sentence the pointer uncovers.
    pub fn with_hint(mut self, hint: &str) -> Self {
        self.hint = Some(hint.to_string());
        self
    }

    /// The same row, which has to be answered.
    pub fn and_required(mut self) -> Self {
        self.required = true;
        self
    }
}

/// What a script wants to know.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Form {
    /// Name this form is known by inside its script.
    ///
    /// What was answered is kept under it, so a script asking two things keeps
    /// two files and neither of them answers the other. A form of no name of
    /// its own is kept under one the application picks.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// True when what is answered here is not to be kept.
    ///
    /// A form offering what a device holds now is such a form: the answer is
    /// about this moment, and the next run has to ask again.
    #[serde(default, skip_serializing_if = "is_not")]
    pub unsaved: bool,
    /// What the window is called.
    #[serde(default)]
    pub title: String,
    /// The sentence under the title, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// The rows, in the order they are asked.
    #[serde(default)]
    pub fields: Vec<Field>,
    /// What the button that goes on says, when it says something of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accept: Option<String>,
    /// What the button that gives up says, when it says something of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject: Option<String>,
}

impl Form {
    /// A form with a title and no rows.
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            ..Self::default()
        }
    }

    /// The same form under a name of its own.
    pub fn named(mut self, id: &str) -> Self {
        self.id = id.to_string();
        self
    }

    /// The same form, what it is answered not kept anywhere.
    pub fn and_unsaved(mut self) -> Self {
        self.unsaved = true;
        self
    }

    /// The same form with one more row.
    pub fn and(mut self, field: Field) -> Self {
        self.fields.push(field);
        self
    }

    /// The row of that name, when there is one.
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|field| field.name == name)
    }

    /// What the form answers with before anybody touches it.
    pub fn defaults(&self) -> BTreeMap<String, Value> {
        self.fields
            .iter()
            .filter_map(|field| {
                field
                    .kind
                    .default_value()
                    .map(|value| (field.name.clone(), value))
            })
            .collect()
    }

    /// Answers with an error when a set of answers does not fit this form.
    ///
    /// It is the one check both ends share: a window that enables its button
    /// and a run answered from the command line agree on what counts as
    /// answered, so a script never has to ask twice.
    pub fn check(&self, answers: &BTreeMap<String, Value>) -> crate::Result<()> {
        for field in self.fields.iter().filter(|field| field.required) {
            let missing = || crate::ScriptError::UnsetVariable {
                name: field.name.clone(),
            };
            let value = answers.get(&field.name).ok_or_else(missing)?;
            if value.is_empty() {
                return Err(missing());
            }
        }
        Ok(())
    }
}

/// What one field answers with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    /// A switch.
    Flag(bool),
    /// Text, of one line or of several.
    Text(String),
    /// Several names of a list.
    Many(Vec<String>),
    /// One name of a list.
    One(String),
}

impl Value {
    /// The text of this answer, whatever shape it has.
    pub fn text(&self) -> String {
        match self {
            Self::Text(text) | Self::One(text) => text.clone(),
            Self::Flag(flag) => flag.to_string(),
            Self::Many(names) => names.join(" "),
        }
    }

    /// True when the answer is a switch that is on, or anything else that is
    /// not empty.
    pub fn flag(&self) -> bool {
        match self {
            Self::Flag(flag) => *flag,
            Self::Text(text) | Self::One(text) => !text.is_empty(),
            Self::Many(names) => !names.is_empty(),
        }
    }

    /// The names this answer carries: one for a pick, several for a list.
    pub fn names(&self) -> Vec<String> {
        match self {
            Self::Many(names) => names.clone(),
            Self::One(name) | Self::Text(name) => vec![name.clone()],
            Self::Flag(_) => Vec::new(),
        }
    }

    /// True when nothing was answered.
    ///
    /// A switch is never empty: off is an answer.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Flag(_) => false,
            Self::Text(text) | Self::One(text) => text.trim().is_empty(),
            Self::Many(names) => names.is_empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modes() -> Vec<Choice> {
        vec![Choice::named("b64", "base64"), Choice::named("raw", "raw")]
    }

    #[test]
    fn one_of_several_always_answers_with_one_of_them() {
        let kind = FieldKind::OneOf {
            options: modes(),
            value: String::new(),
        };
        assert_eq!(kind.default_value(), Some(Value::One("b64".to_string())));

        let kind = FieldKind::OneOf {
            options: modes(),
            value: "nothing of the sort".to_string(),
        };
        assert_eq!(kind.default_value(), Some(Value::One("b64".to_string())));
    }

    #[test]
    fn a_pick_from_a_menu_may_answer_with_nothing() {
        let kind = FieldKind::Select {
            options: modes(),
            value: None,
        };
        assert_eq!(kind.default_value(), None);
    }

    #[test]
    fn a_name_no_entry_carries_is_dropped_from_a_many() {
        let kind = FieldKind::ManyOf {
            options: modes(),
            value: vec!["raw".to_string(), "gone".to_string()],
        };
        assert_eq!(
            kind.default_value(),
            Some(Value::Many(vec!["raw".to_string()]))
        );
    }

    #[test]
    fn a_switch_that_is_off_is_still_an_answer() {
        assert!(!Value::Flag(false).is_empty());
        assert!(Value::Text("  ".to_string()).is_empty());
        assert!(Value::Many(Vec::new()).is_empty());
    }

    #[test]
    fn a_required_row_left_empty_is_refused_with_its_name() {
        let form = Form::new("ask").and(
            Field::new(
                "host",
                FieldKind::Text {
                    value: String::new(),
                    password: false,
                },
            )
            .and_required(),
        );

        let error = form
            .check(&form.defaults())
            .expect_err("an empty answer is refused");
        assert!(matches!(
            error,
            crate::ScriptError::UnsetVariable { name } if name == "host"
        ));

        let mut answers = form.defaults();
        answers.insert("host".to_string(), Value::Text("board".to_string()));
        assert!(form.check(&answers).is_ok());
    }

    #[test]
    fn a_decoration_answers_with_nothing_and_asks_for_nothing() {
        let form = Form::new("ask")
            .and(Field::new("said", FieldKind::Note))
            .and(Field::new("line", FieldKind::Separator));
        assert!(form.defaults().is_empty());
        assert!(form.fields.iter().all(|field| field.kind.is_decoration()));
    }

    #[test]
    fn a_form_says_what_it_is_kept_under_and_whether_it_is_kept() {
        let plain = Form::new("ask");
        assert!(plain.id.is_empty(), "a form of no name of its own");
        assert!(!plain.unsaved, "a form is kept unless it says otherwise");

        let named = Form::new("ask").named("options").and_unsaved();
        assert_eq!(named.id, "options");
        assert!(named.unsaved);

        let written = serde_yaml_ng::to_string(&plain).expect("it is written");
        assert!(
            !written.contains("id:") && !written.contains("unsaved:"),
            "neither is written down while it says nothing: {written}"
        );
        let written = serde_yaml_ng::to_string(&named).expect("it is written");
        assert_eq!(
            serde_yaml_ng::from_str::<Form>(&written).expect("it is read"),
            named
        );
    }

    #[test]
    fn every_kind_is_written_under_its_own_word() {
        let kinds = [
            FieldKind::Text {
                value: String::new(),
                password: false,
            },
            FieldKind::Textarea {
                value: String::new(),
                rows: 1,
            },
            FieldKind::Switch { value: false },
            FieldKind::OneOf {
                options: modes(),
                value: String::new(),
            },
            FieldKind::Select {
                options: modes(),
                value: None,
            },
            FieldKind::ManyOf {
                options: modes(),
                value: Vec::new(),
            },
            FieldKind::Note,
            FieldKind::Separator,
        ];
        let mut names: Vec<&str> = kinds.iter().map(FieldKind::name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), kinds.len(), "two kinds share a word");
    }
}
