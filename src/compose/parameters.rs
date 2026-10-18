//! Compose parameters: what a value of each type accepts, how the create form may show it, and
//! the text the setup script receives for it. Every value reaches the script as a container
//! variable, so whatever its type, a value is text here, in the form the script reads it. A
//! volume parameter is the one whose value is not what its variable holds: the value names a
//! volume, and the script reads where it is mounted.
use crate::model::{mount_target, volume_name};
use anyhow::{Context, Result, anyhow, bail};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const VALUE_LIMIT: usize = 16_384;
const OPTION_LIMIT: usize = 100;
const OPTION_VALUE_LIMIT: usize = 1024;
const DESCRIPTION_LIMIT: usize = 500;
const PATTERN_LIMIT: usize = 500;
const PATTERN_MESSAGE_LIMIT: usize = 200;
/// The memory a compiled pattern may take. A compose can come from an archive or a pasted
/// configuration, and a short pattern such as `\w{1,100}` compiles to more than this.
const PATTERN_MEMORY: usize = 4 << 20;
/// Variables the setup script, its shell or the desktop session rely on.
const RESERVED: &[&str] = &[
    "CDPATH", "DISPLAY", "ENV", "HOME", "HOSTNAME", "IFS", "LANG", "LOGNAME", "OLDPWD", "PATH",
    "PS4", "PWD", "SHELL", "SHLVL", "TERM", "TMPDIR", "TZ", "USER",
];
const RESERVED_PREFIXES: &[&str] = &[
    "BASH",
    "BILSKIRNIR",
    "DBUS_",
    "DESKTOP_",
    "GPU_",
    "LC_",
    "LD_",
    "PULSE_",
    "PYTHON",
    "XDG_",
];

/// What a parameter's value is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Any text. The only type that can be secret.
    #[default]
    Text,
    /// A plain decimal number such as `42` or `-1.5`.
    Number,
    /// `true` or `false`.
    Boolean,
    /// One of the options, or other text when `allow_custom` is set.
    Choice,
    /// Any of the options, one per line.
    Choices,
    /// A named volume mounted at the parameter's `path`, or empty for none. The script receives
    /// the path, and the volume's name in a second variable.
    Volume,
    /// An absolute folder of the desktop outside its system folders, such as the project an
    /// editor opens. The script creates what it needs there, and uploads may go into it.
    Folder,
}

/// How the create form shows a parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Display {
    Input,
    Multiline,
    Slider,
    Toggle,
    Checkbox,
    Select,
    Radio,
    Checkboxes,
}

impl Kind {
    /// The ways the create form can show a value of this type; the first one is the default.
    pub fn displays(self) -> &'static [Display] {
        match self {
            Kind::Text => &[Display::Input, Display::Multiline],
            Kind::Number => &[Display::Input, Display::Slider],
            Kind::Boolean => &[Display::Toggle, Display::Checkbox],
            Kind::Choice => &[Display::Select, Display::Radio],
            Kind::Choices => &[Display::Checkboxes],
            Kind::Volume => &[Display::Select],
            Kind::Folder => &[Display::Input],
        }
    }
}

/// One option of a choice parameter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    /// What the script receives.
    pub value: String,
    /// What the form shows; the value when empty.
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    /// The environment variable that carries the value.
    pub name: String,
    /// What the create form asks for; the name when empty.
    #[serde(default)]
    pub label: String,
    /// Help the create form shows with the field, such as where to find the value.
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "type")]
    pub kind: Kind,
    /// How the create form shows it; each type has its own choices and default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<Display>,
    /// A boolean that is required must be turned on.
    #[serde(default)]
    pub required: bool,
    /// A password or token: masked in logs and never returned.
    #[serde(default)]
    pub secret: bool,
    /// In the form the script receives it.
    #[serde(default)]
    pub default: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<Choice>,
    /// A choice that also takes text other than its options.
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_custom: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    /// Where a volume parameter's volume is mounted in the desktop, fixed by the compose.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    /// A regular expression a text value must match as a whole.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pattern: String,
    /// What the create form says when a value does not match the pattern.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pattern_message: String,
}
fn is_false(value: &bool) -> bool {
    !value
}

impl Parameter {
    pub(super) fn title(&self) -> &str {
        if self.label.is_empty() {
            &self.name
        } else {
            &self.label
        }
    }

    fn display(&self) -> Display {
        self.display.unwrap_or(self.kind.displays()[0])
    }

    pub(super) fn validate(&self) -> Result<()> {
        name(&self.name)?;
        let name = &self.name;
        if !one_line(&self.label, 100) {
            bail!("A parameter label is one line of at most 100 characters: {name}");
        }
        if self.description.chars().count() > DESCRIPTION_LIMIT || self.description.contains('\0') {
            bail!(
                "A parameter description is at most {DESCRIPTION_LIMIT} characters without NUL characters: {name}"
            );
        }
        if let Some(display) = self.display
            && !self.kind.displays().contains(&display)
        {
            bail!(
                "A {} parameter cannot be shown as {}: {name}",
                wire(self.kind),
                wire(display)
            );
        }
        if self.secret {
            if self.kind != Kind::Text {
                bail!("Only text parameters can be secret: {name}");
            }
            if !self.default.is_empty() {
                bail!(
                    "The secret parameter {name} cannot have a default, since the image record would keep it"
                );
            }
        }
        if matches!(self.kind, Kind::Choice | Kind::Choices) {
            self.check_options()?;
        } else if !self.options.is_empty() {
            bail!("Only choice parameters have options: {name}");
        }
        if self.allow_custom && self.kind != Kind::Choice {
            bail!("Only a single choice can allow other values: {name}");
        }
        if self.kind == Kind::Number {
            self.check_range()?;
        } else if self.min.is_some() || self.max.is_some() || self.step.is_some() {
            bail!("Only number parameters have min, max and step: {name}");
        }
        if self.kind == Kind::Volume {
            mount_target(&self.path).with_context(|| {
                format!(
                    "The path of {name} must be an absolute folder outside the desktop's system folders, such as /opt/unity"
                )
            })?;
        } else if !self.path.is_empty() {
            bail!("Only volume parameters have a path: {name}");
        }
        if self.kind == Kind::Text {
            self.check_pattern()?;
        } else if !self.pattern.is_empty() || !self.pattern_message.is_empty() {
            bail!("Only text parameters have a pattern: {name}");
        }
        self.check(&self.default)
            .with_context(|| format!("The default of {name} is not a value it accepts"))
    }

    fn check_options(&self) -> Result<()> {
        let name = &self.name;
        if self.options.is_empty() || self.options.len() > OPTION_LIMIT {
            bail!("A choice parameter has 1 to {OPTION_LIMIT} options: {name}");
        }
        let mut values = HashSet::new();
        for option in &self.options {
            if option.value.is_empty() || !one_line(&option.value, OPTION_VALUE_LIMIT) {
                bail!(
                    "An option value is one line of 1 to {OPTION_VALUE_LIMIT} characters: {name}"
                );
            }
            if !one_line(&option.label, 100) {
                bail!("An option label is one line of at most 100 characters: {name}");
            }
            if !values.insert(&option.value) {
                bail!("The options of {name} repeat {}", option.value);
            }
        }
        Ok(())
    }

    fn check_range(&self) -> Result<()> {
        let name = &self.name;
        for (field, bound) in [("min", self.min), ("max", self.max), ("step", self.step)] {
            if bound.is_some_and(|value| !value.is_finite()) {
                bail!("The {field} of {name} must be a finite number");
            }
        }
        if self.step.is_some_and(|step| step <= 0.0) {
            bail!("The step of {name} must be positive");
        }
        if let (Some(min), Some(max)) = (self.min, self.max)
            && min > max
        {
            bail!("The min of {name} is above its max");
        }
        if self.display() == Display::Slider && (self.min.is_none() || self.max.is_none()) {
            bail!("A slider needs a min and a max: {name}");
        }
        Ok(())
    }

    fn check_pattern(&self) -> Result<()> {
        let name = &self.name;
        if !one_line(&self.pattern_message, PATTERN_MESSAGE_LIMIT) {
            bail!(
                "A pattern message is one line of at most {PATTERN_MESSAGE_LIMIT} characters: {name}"
            );
        }
        if self.pattern.is_empty() && !self.pattern_message.is_empty() {
            bail!("{name} has a pattern message but no pattern");
        }
        self.pattern().map(drop)
    }

    /// The pattern as it is checked, matching a whole value, or `None` without one. The value
    /// can be 16 KiB, and the regex crate matches in time linear to it whatever the pattern.
    fn pattern(&self) -> Result<Option<Regex>> {
        if self.pattern.is_empty() {
            return Ok(None);
        }
        let name = &self.name;
        if !one_line(&self.pattern, PATTERN_LIMIT) {
            bail!("A pattern is one line of at most {PATTERN_LIMIT} characters: {name}");
        }
        let compile = |pattern: &str| {
            RegexBuilder::new(pattern)
                .size_limit(PATTERN_MEMORY)
                .build()
                .map_err(|error| pattern_error(name, error))
        };
        // On its own first, so a pattern such as `a)|(b` cannot close the group around it.
        compile(&self.pattern)?;
        compile(&format!(r"\A(?:{})\z", self.pattern)).map(Some)
    }

    /// Whether this parameter accepts `value`. An empty value is left to `required`.
    pub(super) fn check(&self, value: &str) -> Result<()> {
        if value.len() > VALUE_LIMIT || value.contains('\0') {
            bail!(
                "The value of {} must be at most {VALUE_LIMIT} bytes without NUL characters",
                self.name
            );
        }
        if value.is_empty() {
            return Ok(());
        }
        let title = self.title();
        let lines = value.contains(['\n', '\r']);
        match self.kind {
            Kind::Text if lines && self.display() != Display::Multiline => {
                bail!("{title} must be one line")
            }
            // The value itself stays out of the error, since a secret can have a pattern too.
            Kind::Text => match self.pattern()? {
                Some(pattern) if !pattern.is_match(value) => match &*self.pattern_message {
                    "" => bail!("{title} does not have the expected format"),
                    message => bail!("{title} does not have the expected format: {message}"),
                },
                _ => {}
            },
            Kind::Number => {
                let number = decimal(value)
                    .with_context(|| format!("{title} must be a number such as 42 or -1.5"))?;
                if let Some(min) = self.min
                    && number < min
                {
                    bail!("{title} must be at least {min}");
                }
                if let Some(max) = self.max
                    && number > max
                {
                    bail!("{title} must be at most {max}");
                }
                if let Some(step) = self.step {
                    let from = self.min.unwrap_or(0.0);
                    let steps = (number - from) / step;
                    if (steps - steps.round()).abs() > 1e-9 * steps.abs().max(1.0) {
                        bail!("{title} must go up from {from} in steps of {step}");
                    }
                }
            }
            Kind::Boolean if value != "true" && value != "false" => {
                bail!("{title} must be true or false")
            }
            Kind::Boolean => {}
            Kind::Choice if lines => bail!("{title} must be one line"),
            Kind::Choice if !self.allow_custom && !self.has_option(value) => {
                bail!("{title} must be one of its options")
            }
            Kind::Choice => {}
            Kind::Choices => {
                let mut seen = HashSet::new();
                if !value
                    .split('\n')
                    .all(|line| self.has_option(line) && seen.insert(line))
                {
                    bail!("{title} takes its options one per line, each at most once");
                }
            }
            Kind::Volume if volume_name(value).is_err() => bail!(
                "{title} must name a volume: 2 to 128 letters, digits, dots, dashes and underscores, starting with a letter or digit"
            ),
            Kind::Volume => {}
            Kind::Folder if lines || crate::model::desktop_folder(value).is_err() => bail!(
                "{title} must be an absolute folder outside the desktop's system folders, such as /home/desktop/Projects"
            ),
            Kind::Folder => {}
        }
        Ok(())
    }

    fn has_option(&self, value: &str) -> bool {
        self.options.iter().any(|option| option.value == value)
    }

    /// The second variable of a volume parameter, which holds the volume's name.
    pub(super) fn volume_variable(&self) -> Option<String> {
        (self.kind == Kind::Volume).then(|| format!("{}_VOLUME", self.name))
    }

    /// The variables the setup script receives for `value`: the value itself, except for a
    /// volume parameter, whose variable holds the path and a second one the volume, if any.
    pub(super) fn environment(&self, value: &str) -> Vec<(String, String)> {
        match self.volume_variable() {
            Some(volume) => vec![
                (self.name.clone(), self.path.clone()),
                (volume, value.into()),
            ],
            None => vec![(self.name.clone(), value.into())],
        }
    }

    /// The folder this parameter names for `value`: a volume parameter's path, which the desktop
    /// user is given whether a volume was mounted there or not, and a folder parameter's value.
    pub(super) fn place(&self, value: &str) -> Option<super::Place> {
        let path = match self.kind {
            Kind::Volume => self.path.clone(),
            Kind::Folder if !value.is_empty() => value.into(),
            _ => return None,
        };
        Some(super::Place {
            label: self.title().into(),
            path,
        })
    }

    /// Where this parameter mounts `value`, a volume parameter given a volume.
    pub(super) fn mount(&self, value: &str) -> Option<crate::model::Mount> {
        (self.kind == Kind::Volume && !value.is_empty()).then(|| crate::model::Mount {
            volume: value.into(),
            target: self.path.clone(),
            readonly: false,
        })
    }

    /// The value a new container gives the script: the one given, otherwise the default. A
    /// boolean without one is `false`.
    pub(super) fn resolve(&self, given: Option<&String>) -> Result<String> {
        let mut value = given.unwrap_or(&self.default).clone();
        if self.kind == Kind::Boolean && value.is_empty() {
            value = "false".into();
        }
        if self.required {
            let title = self.title();
            match self.kind {
                Kind::Boolean if value != "true" => bail!("Turn on {title}"),
                Kind::Choice | Kind::Choices if value.is_empty() => {
                    bail!("Choose a value for {title}")
                }
                Kind::Volume if value.is_empty() => bail!("Choose a volume for {title}"),
                Kind::Text | Kind::Number | Kind::Folder if value.is_empty() => {
                    bail!("Enter a value for {title}")
                }
                _ => {}
            }
        }
        self.check(&value)?;
        Ok(value)
    }
}

fn name(name: &str) -> Result<()> {
    let mut bytes = name.bytes();
    if name.len() > 64
        || !bytes.next().is_some_and(|b| b.is_ascii_uppercase())
        || !bytes.all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    {
        bail!(
            "Parameter names are environment variables of at most 64 capital letters, digits and underscores, starting with a letter: {name}"
        );
    }
    if RESERVED.contains(&name) || RESERVED_PREFIXES.iter().any(|p| name.starts_with(p)) {
        bail!("{name} is used by the desktop or the shell; choose another parameter name");
    }
    Ok(())
}

/// A type or display as requests spell it.
fn wire(value: impl Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Why a pattern does not compile, without the regex crate's lines that point at the spot.
fn pattern_error(name: &str, error: regex::Error) -> anyhow::Error {
    match error {
        regex::Error::CompiledTooBig(_) => anyhow!(
            "The pattern of {name} takes too much memory once compiled; repeat less, or use ASCII classes such as [A-Za-z0-9_] instead of \\w"
        ),
        error => {
            let text = error.to_string();
            let reason = text
                .lines()
                .rev()
                .find_map(|line| line.strip_prefix("error: "))
                .unwrap_or(text.trim());
            anyhow!("The pattern of {name} is not a regular expression the manager reads: {reason}")
        }
    }
}

fn one_line(text: &str, limit: usize) -> bool {
    text.chars().count() <= limit && !text.contains(['\n', '\r', '\0'])
}

/// A plain decimal number, in the form scripts and most tools read: an optional minus sign,
/// digits and an optional fraction, without exponents or spaces.
fn decimal(value: &str) -> Option<f64> {
    let digits = value.strip_prefix('-').unwrap_or(value);
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, "0"));
    if whole.is_empty()
        || fraction.is_empty()
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    value.parse().ok().filter(|number: &f64| number.is_finite())
}

#[cfg(test)]
#[path = "parameters_tests.rs"]
mod tests;
