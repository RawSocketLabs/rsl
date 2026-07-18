use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process;

type TaskResult<T> = Result<T, Box<dyn Error>>;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const PROFILES: &[&str] = &[
    "public-library",
    "internal-library",
    "performance-application",
    "pragmatic-application",
    "prototype",
];
const SKILLS: &[&str] = &["rsl-rust-core", "rsl-rust-review"];
const DOMAINS: &[&str] = &["protocol", "dsp", "systems"];

#[derive(Debug)]
struct TaskError(String);

impl fmt::Display for TaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for TaskError {}

#[derive(Debug, Default)]
struct ComponentAdoption {
    profile: String,
    domains: Vec<String>,
}

#[derive(Debug, Default)]
struct Adoption {
    schema: u32,
    standards: String,
    profile: String,
    skills: Vec<String>,
    components: BTreeMap<String, ComponentAdoption>,
}

#[derive(Debug, Default)]
struct EvalCase {
    schema: u32,
    id: String,
    class: String,
    skill: String,
    profile: String,
    prompt: String,
    fixture: String,
    grader: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InstallAgent {
    Common,
    Claude,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InstallScope {
    Repo,
    User,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        process::exit(1);
    }
}

fn run() -> TaskResult<()> {
    let mut arguments = env::args().skip(1);
    let Some(command) = arguments.next() else {
        return usage_error();
    };
    let remaining: Vec<String> = arguments.collect();

    match command.as_str() {
        "validate" => command_validate(&remaining),
        "generate" => command_generate(&remaining),
        "inspect-adoption" => command_inspect_adoption(&remaining),
        "install" => command_install(&remaining),
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        unknown => fail(format!("unknown command `{unknown}`")),
    }
}

fn command_validate(arguments: &[String]) -> TaskResult<()> {
    let (root, flags) = parse_root_and_flags(arguments)?;
    if !flags.is_empty() {
        return fail(format!("unexpected validate argument `{}`", flags[0]));
    }
    validate_source(&root)?;
    check_generated(&root)?;
    println!("validated {}", root.display());
    Ok(())
}

fn command_generate(arguments: &[String]) -> TaskResult<()> {
    let (root, flags) = parse_root_and_flags(arguments)?;
    let mut check = false;
    for flag in flags {
        match flag.as_str() {
            "--check" if !check => check = true,
            unknown => return fail(format!("unexpected generate argument `{unknown}`")),
        }
    }

    validate_source(&root)?;
    if check {
        check_generated(&root)?;
        println!("generated adapters match canonical sources");
    } else {
        write_generated(&root)?;
        check_generated(&root)?;
        println!("generated adapters in {}", root.join("generated").display());
    }
    Ok(())
}

fn command_inspect_adoption(arguments: &[String]) -> TaskResult<()> {
    if arguments.len() != 1 {
        return fail("inspect-adoption requires exactly one repository path");
    }
    let repository = canonical_directory(Path::new(&arguments[0]))?;
    let adoption_path = repository.join("rsl-rust-standards.toml");
    let adoption_text = read_text(&adoption_path)?;
    let adoption = parse_adoption(&adoption_text, &adoption_path)?;

    println!("repository: {}", repository.display());
    println!("standards: {}", adoption.standards);
    println!("profile: {}", adoption.profile);
    println!("skills: {}", adoption.skills.join(", "));
    if adoption.components.is_empty() {
        println!("components: none");
    } else {
        for (path, component) in &adoption.components {
            println!(
                "component {path}: profile={}, domains={}",
                component.profile,
                component.domains.join(",")
            );
        }
    }

    report_adoption_files(&repository, &adoption);
    Ok(())
}

fn command_install(arguments: &[String]) -> TaskResult<()> {
    let mut agent = None;
    let mut scope = None;
    let mut target = None;
    let mut replace = false;
    let mut index = 0;

    while index < arguments.len() {
        match arguments[index].as_str() {
            "--agent" => {
                let value = required_value(arguments, index, "--agent")?;
                agent = Some(match value {
                    "common" => InstallAgent::Common,
                    "claude" => InstallAgent::Claude,
                    "multi-agent" => {
                        return fail(
                            "multi-agent installation is blocked until Cursor duplicate-skill behavior is verified",
                        );
                    }
                    _ => return fail(format!("unknown agent install profile `{value}`")),
                });
                index += 2;
            }
            "--scope" => {
                let value = required_value(arguments, index, "--scope")?;
                scope = Some(match value {
                    "repo" => InstallScope::Repo,
                    "user" => InstallScope::User,
                    _ => return fail(format!("unknown installation scope `{value}`")),
                });
                index += 2;
            }
            "--target" => {
                target = Some(PathBuf::from(required_value(arguments, index, "--target")?));
                index += 2;
            }
            "--replace" if !replace => {
                replace = true;
                index += 1;
            }
            unknown => return fail(format!("unexpected install argument `{unknown}`")),
        }
    }

    let agent = agent.ok_or_else(|| task_error("install requires --agent"))?;
    let scope = scope.ok_or_else(|| task_error("install requires --scope"))?;
    let base = install_base(scope, target)?;
    let root = project_root()?;
    validate_source(&root)?;
    check_generated(&root)?;
    install_generated(&root, &base, agent, scope, replace)
}

fn print_usage() {
    println!(
        "rsl-rust-standards xtask\n\
         \n\
         cargo xtask validate [--root PATH]\n\
         cargo xtask generate [--check] [--root PATH]\n\
         cargo xtask inspect-adoption REPOSITORY\n\
         cargo xtask install --agent common|claude|multi-agent --scope repo|user [--target PATH] [--replace]"
    );
}

fn usage_error<T>() -> TaskResult<T> {
    print_usage();
    fail("a command is required")
}

fn parse_root_and_flags(arguments: &[String]) -> TaskResult<(PathBuf, Vec<String>)> {
    let mut root = None;
    let mut flags = Vec::new();
    let mut index = 0;

    while index < arguments.len() {
        if arguments[index] == "--root" {
            if root.is_some() {
                return fail("--root may be specified only once");
            }
            root = Some(PathBuf::from(required_value(arguments, index, "--root")?));
            index += 2;
        } else {
            flags.push(arguments[index].clone());
            index += 1;
        }
    }

    let root = match root {
        Some(path) => canonical_directory(&path)?,
        None => project_root()?,
    };
    Ok((root, flags))
}

fn required_value<'a>(arguments: &'a [String], index: usize, flag: &str) -> TaskResult<&'a str> {
    arguments
        .get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| task_error(format!("{flag} requires a value")))
}

fn project_root() -> TaskResult<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| task_error("could not resolve the standards repository root"))?;
    canonical_directory(root)
}

fn canonical_directory(path: &Path) -> TaskResult<PathBuf> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        task_error(format!(
            "could not resolve directory {}: {error}",
            path.display()
        ))
    })?;
    if !canonical.is_dir() {
        return fail(format!("{} is not a directory", canonical.display()));
    }
    Ok(canonical)
}

fn validate_source(root: &Path) -> TaskResult<()> {
    for relative in [
        "Cargo.toml",
        "LICENSE-APACHE",
        "LICENSE-MIT",
        "docs/source-ledger.md",
        "docs/authoring-conventions.md",
        "docs/evaluation-guide.md",
        "templates/AGENTS.root.md",
        "templates/AGENTS.nested.md",
        "templates/CLAUDE.md.template",
        "templates/rsl-rust-standards.toml",
    ] {
        require_file(&root.join(relative))?;
    }

    validate_licenses(root)?;
    validate_skills(root)?;
    let adoption_path = root.join("templates/rsl-rust-standards.toml");
    let adoption_text = read_text(&adoption_path)?;
    let adoption = parse_adoption(&adoption_text, &adoption_path)?;
    if adoption.standards != VERSION {
        return fail(format!(
            "{} pins standards {}, but xtask is version {VERSION}",
            adoption_path.display(),
            adoption.standards
        ));
    }
    validate_evals(root)?;
    Ok(())
}

fn validate_evals(root: &Path) -> TaskResult<()> {
    let evals_root = root.join("evals");
    let case_paths: Vec<PathBuf> = recursive_files(&evals_root)?
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "case.toml"))
        .collect();
    if case_paths.len() < 4 {
        return fail("eval suite must contain at least four cases");
    }

    let mut ids = BTreeSet::new();
    let mut classes = BTreeSet::new();
    for case_path in case_paths {
        let case = parse_eval_case(&read_text(&case_path)?, &case_path)?;
        if !ids.insert(case.id.clone()) {
            return fail(format!("duplicate eval ID `{}`", case.id));
        }
        classes.insert(case.class.clone());
        validate_eval_artifacts(&case, &case_path)?;
    }
    for required in ["decision", "review", "precedence", "discovery"] {
        if !classes.contains(required) {
            return fail(format!("eval suite is missing `{required}` coverage"));
        }
    }
    Ok(())
}

fn parse_eval_case(text: &str, path: &Path) -> TaskResult<EvalCase> {
    let mut case = EvalCase::default();
    for (line_index, raw_line) in text.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| {
            task_error(format!(
                "invalid eval metadata at {}:{}",
                path.display(),
                line_index + 1
            ))
        })?;
        let key = key.trim();
        let value = value.trim();
        match key {
            "schema" => {
                case.schema = value.parse().map_err(|error| {
                    task_error(format!(
                        "invalid eval schema in {}: {error}",
                        path.display()
                    ))
                })?;
            }
            "id" => case.id = parse_string(value, path)?,
            "class" => case.class = parse_string(value, path)?,
            "skill" => case.skill = parse_string(value, path)?,
            "profile" => case.profile = parse_string(value, path)?,
            "prompt" => case.prompt = parse_string(value, path)?,
            "fixture" => case.fixture = parse_string(value, path)?,
            "grader" => case.grader = parse_string(value, path)?,
            _ => return fail(format!("unknown eval key `{key}` in {}", path.display())),
        }
    }

    if case.schema != 1 {
        return fail(format!("{} must use eval schema 1", path.display()));
    }
    if !valid_eval_id(&case.id) {
        return fail(format!(
            "invalid eval ID `{}` in {}",
            case.id,
            path.display()
        ));
    }
    if !matches!(
        case.class.as_str(),
        "trigger" | "decision" | "review" | "precedence" | "discovery" | "cost" | "regression"
    ) {
        return fail(format!(
            "unknown eval class `{}` in {}",
            case.class,
            path.display()
        ));
    }
    if case.skill != "none" && !SKILLS.contains(&case.skill.as_str()) {
        return fail(format!(
            "unknown eval skill `{}` in {}",
            case.skill,
            path.display()
        ));
    }
    if !PROFILES.contains(&case.profile.as_str()) {
        return fail(format!(
            "unknown eval profile `{}` in {}",
            case.profile,
            path.display()
        ));
    }
    Ok(case)
}

fn valid_eval_id(value: &str) -> bool {
    valid_skill_name(value)
}

fn validate_eval_artifacts(case: &EvalCase, case_path: &Path) -> TaskResult<()> {
    let case_root = case_path
        .parent()
        .ok_or_else(|| task_error(format!("invalid eval path {}", case_path.display())))?;
    let prompt = confined_case_path(case_root, &case.prompt, case_path)?;
    let fixture = confined_case_path(case_root, &case.fixture, case_path)?;
    let grader = confined_case_path(case_root, &case.grader, case_path)?;
    require_file(&prompt)?;
    if !fixture.is_dir() {
        return fail(format!(
            "missing eval fixture directory {}",
            fixture.display()
        ));
    }
    require_file(&grader)?;

    let prompt_text = read_text(&prompt)?;
    if prompt_text.contains("Expected observations")
        || prompt_text.contains("Forbidden behavior")
        || prompt_text.contains("Scoring")
    {
        return fail(format!(
            "eval prompt {} leaks grader content",
            prompt.display()
        ));
    }
    let grader_text = read_text(&grader)?;
    if !grader_text.contains("# Grader") {
        return fail(format!(
            "{} is missing its grader heading",
            grader.display()
        ));
    }
    Ok(())
}

fn confined_case_path(case_root: &Path, value: &str, metadata: &Path) -> TaskResult<PathBuf> {
    if value.is_empty() || component_path_escapes(value) {
        return fail(format!(
            "invalid eval artifact path `{value}` in {}",
            metadata.display()
        ));
    }
    Ok(case_root.join(value))
}

fn validate_licenses(root: &Path) -> TaskResult<()> {
    let apache = read_text(&root.join("LICENSE-APACHE"))?;
    let mit = read_text(&root.join("LICENSE-MIT"))?;
    if !apache.contains("Apache License") || !apache.contains("Version 2.0") {
        return fail("LICENSE-APACHE does not contain the Apache-2.0 license text");
    }
    if !mit.contains("Permission is hereby granted") {
        return fail("LICENSE-MIT does not contain the MIT license text");
    }
    Ok(())
}

fn validate_skills(root: &Path) -> TaskResult<()> {
    let skills_root = root.join("skills");
    let directories = direct_directories(&skills_root)?;
    let expected: BTreeSet<String> = SKILLS.iter().map(ToString::to_string).collect();
    let actual: BTreeSet<String> = directories
        .iter()
        .filter_map(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    if actual != expected {
        return fail(format!(
            "canonical skill set mismatch: expected {expected:?}, found {actual:?}"
        ));
    }

    let mut names = BTreeSet::new();
    let mut rule_ids = BTreeSet::new();
    for directory in directories {
        validate_skill(&directory, &mut names, &mut rule_ids)?;
    }
    Ok(())
}

fn validate_skill(
    directory: &Path,
    names: &mut BTreeSet<String>,
    rule_ids: &mut BTreeSet<String>,
) -> TaskResult<()> {
    let directory_name = directory
        .file_name()
        .ok_or_else(|| task_error(format!("invalid skill directory {}", directory.display())))?
        .to_string_lossy()
        .into_owned();
    if !valid_skill_name(&directory_name) {
        return fail(format!("invalid skill directory name `{directory_name}`"));
    }

    let skill_path = directory.join("SKILL.md");
    let skill_text = read_text(&skill_path)?;
    let frontmatter = parse_frontmatter(&skill_text, &skill_path)?;
    let name = frontmatter
        .get("name")
        .ok_or_else(|| task_error(format!("{} is missing `name`", skill_path.display())))?;
    let description = frontmatter
        .get("description")
        .ok_or_else(|| task_error(format!("{} is missing `description`", skill_path.display())))?;
    if name != &directory_name {
        return fail(format!(
            "skill name `{name}` does not match directory `{directory_name}`"
        ));
    }
    if description.len() < 40 || description.contains("TODO") {
        return fail(format!(
            "skill `{name}` needs a complete trigger description"
        ));
    }
    if !names.insert(name.clone()) {
        return fail(format!("duplicate skill name `{name}`"));
    }

    let references_root = directory.join("references");
    let references = direct_files(&references_root)?;
    let linked = reference_links(&skill_text);
    for link in &linked {
        require_file(&directory.join(link))?;
    }
    let actual_references: BTreeSet<String> = references
        .iter()
        .filter_map(|path| path.file_name())
        .map(|name| format!("references/{}", name.to_string_lossy()))
        .collect();
    if linked != actual_references {
        return fail(format!(
            "skill `{name}` reference links differ from files: linked={linked:?}, files={actual_references:?}"
        ));
    }
    reject_nested_directories(&references_root)?;

    for reference in references {
        let text = read_text(&reference)?;
        validate_rule_blocks(&text, &reference, rule_ids)?;
    }
    validate_openai_yaml(directory, name)
}

fn parse_frontmatter(text: &str, path: &Path) -> TaskResult<BTreeMap<String, String>> {
    let mut lines = text.lines();
    if lines.next() != Some("---") {
        return fail(format!(
            "{} must begin with YAML frontmatter",
            path.display()
        ));
    }

    let mut values = BTreeMap::new();
    let mut closed = false;
    for line in lines {
        if line == "---" {
            closed = true;
            break;
        }
        let (key, value) = line.split_once(':').ok_or_else(|| {
            task_error(format!(
                "invalid frontmatter line in {}: {line}",
                path.display()
            ))
        })?;
        let key = key.trim();
        if !matches!(key, "name" | "description") {
            return fail(format!(
                "unsupported frontmatter key `{key}` in {}",
                path.display()
            ));
        }
        let value = value.trim();
        if value.is_empty() {
            return fail(format!("empty `{key}` in {}", path.display()));
        }
        if values.insert(key.to_owned(), value.to_owned()).is_some() {
            return fail(format!("duplicate `{key}` in {}", path.display()));
        }
    }
    if !closed {
        return fail(format!("unclosed frontmatter in {}", path.display()));
    }
    Ok(values)
}

fn valid_skill_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn reference_links(text: &str) -> BTreeSet<String> {
    let mut links = BTreeSet::new();
    let mut remaining = text;
    while let Some(offset) = remaining.find("references/") {
        remaining = &remaining[offset..];
        let end = remaining
            .find(|character: char| {
                !(character.is_ascii_alphanumeric() || matches!(character, '/' | '.' | '-' | '_'))
            })
            .unwrap_or(remaining.len());
        links.insert(remaining[..end].to_owned());
        remaining = &remaining[end..];
    }
    links
}

fn validate_rule_blocks(
    text: &str,
    path: &Path,
    known_ids: &mut BTreeSet<String>,
) -> TaskResult<()> {
    let lines: Vec<&str> = text.lines().collect();
    let starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| line.strip_prefix("### ").map(|_| index))
        .collect();

    for (position, start) in starts.iter().enumerate() {
        let heading = lines[*start].trim_start_matches("### ");
        let Some(id) = heading.split_whitespace().next() else {
            continue;
        };
        if !is_rule_id(id) {
            continue;
        }
        if !known_ids.insert(id.to_owned()) {
            return fail(format!("duplicate rule ID `{id}` in {}", path.display()));
        }
        let end = starts.get(position + 1).copied().unwrap_or(lines.len());
        let block = lines[*start + 1..end].join("\n");
        for field in [
            "- **Strength:**",
            "- **Applies to:**",
            "- **Directive:**",
            "- **Exceptions:**",
            "- **Mechanical owner:**",
            "- **Sources:**",
        ] {
            if !block.contains(field) {
                return fail(format!(
                    "rule `{id}` in {} is missing `{field}`",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

fn is_rule_id(value: &str) -> bool {
    let Some((prefix, suffix)) = value.rsplit_once('-') else {
        return false;
    };
    prefix.contains('-')
        && prefix
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte == b'-')
        && suffix.len() == 3
        && suffix.bytes().all(|byte| byte.is_ascii_digit())
}

fn validate_openai_yaml(directory: &Path, skill_name: &str) -> TaskResult<()> {
    let path = directory.join("agents/openai.yaml");
    let text = read_text(&path)?;
    let display_name = yaml_quoted_value(&text, "display_name", &path)?;
    let short_description = yaml_quoted_value(&text, "short_description", &path)?;
    let default_prompt = yaml_quoted_value(&text, "default_prompt", &path)?;

    if display_name.trim().is_empty() {
        return fail(format!("{} has an empty display name", path.display()));
    }
    let short_length = short_description.chars().count();
    if !(25..=64).contains(&short_length) {
        return fail(format!(
            "{} short_description must contain 25-64 characters",
            path.display()
        ));
    }
    if !default_prompt.contains(&format!("${skill_name}")) {
        return fail(format!(
            "{} default_prompt must mention `${skill_name}`",
            path.display()
        ));
    }
    Ok(())
}

fn yaml_quoted_value(text: &str, key: &str, path: &Path) -> TaskResult<String> {
    let prefix = format!("{key}:");
    let value = text
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(&prefix))
        .ok_or_else(|| task_error(format!("{} is missing `{key}`", path.display())))?
        .trim();
    if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
        return fail(format!("{} `{key}` must be quoted", path.display()));
    }
    Ok(value[1..value.len() - 1].to_owned())
}

fn parse_adoption(text: &str, path: &Path) -> TaskResult<Adoption> {
    let mut adoption = Adoption::default();
    let mut section = None;

    for (line_index, raw_line) in text.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            section = Some(parse_component_header(line, path, line_index + 1)?);
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| {
            task_error(format!(
                "invalid adoption line {}:{}",
                path.display(),
                line_index + 1
            ))
        })?;
        let key = key.trim();
        let value = value.trim();
        if let Some(component_path) = &section {
            let component = adoption
                .components
                .entry(component_path.clone())
                .or_default();
            match key {
                "profile" => component.profile = parse_string(value, path)?,
                "domains" => component.domains = parse_string_array(value, path)?,
                _ => {
                    return fail(format!(
                        "unknown component adoption key `{key}` in {}",
                        path.display()
                    ));
                }
            }
        } else {
            match key {
                "schema" => {
                    adoption.schema = value.parse().map_err(|error| {
                        task_error(format!("invalid schema in {}: {error}", path.display()))
                    })?;
                }
                "standards" => adoption.standards = parse_string(value, path)?,
                "profile" => adoption.profile = parse_string(value, path)?,
                "skills" => adoption.skills = parse_string_array(value, path)?,
                _ => {
                    return fail(format!(
                        "unknown adoption key `{key}` in {}",
                        path.display()
                    ));
                }
            }
        }
    }

    validate_adoption(&adoption, path)?;
    Ok(adoption)
}

fn parse_component_header(line: &str, path: &Path, line_number: usize) -> TaskResult<String> {
    let prefix = "[components.\"";
    let suffix = "\"]";
    if !line.starts_with(prefix) || !line.ends_with(suffix) {
        return fail(format!(
            "unsupported section at {}:{line_number}",
            path.display()
        ));
    }
    let component = &line[prefix.len()..line.len() - suffix.len()];
    if component.is_empty() || component_path_escapes(component) {
        return fail(format!(
            "invalid component path `{component}` in {}",
            path.display()
        ));
    }
    Ok(component.to_owned())
}

fn component_path_escapes(value: &str) -> bool {
    Path::new(value).is_absolute()
        || Path::new(value)
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
}

fn parse_string(value: &str, path: &Path) -> TaskResult<String> {
    if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
        return fail(format!(
            "expected quoted string in {}: {value}",
            path.display()
        ));
    }
    let inner = &value[1..value.len() - 1];
    if inner.contains('"') || inner.contains('\\') {
        return fail(format!(
            "escaped adoption strings are not supported in {}",
            path.display()
        ));
    }
    Ok(inner.to_owned())
}

fn parse_string_array(value: &str, path: &Path) -> TaskResult<Vec<String>> {
    if !value.starts_with('[') || !value.ends_with(']') {
        return fail(format!(
            "expected string array in {}: {value}",
            path.display()
        ));
    }
    let inner = value[1..value.len() - 1].trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    inner
        .split(',')
        .map(str::trim)
        .map(|item| parse_string(item, path))
        .collect()
}

fn validate_adoption(adoption: &Adoption, path: &Path) -> TaskResult<()> {
    if adoption.schema != 1 {
        return fail(format!("{} must use adoption schema 1", path.display()));
    }
    if adoption.standards.is_empty()
        || adoption
            .standards
            .chars()
            .any(|character| matches!(character, '*' | '^' | '~' | '<' | '>'))
    {
        return fail(format!(
            "{} must pin an exact standards release or commit",
            path.display()
        ));
    }
    if !PROFILES.contains(&adoption.profile.as_str()) {
        return fail(format!(
            "unknown default profile `{}` in {}",
            adoption.profile,
            path.display()
        ));
    }
    if adoption.skills.is_empty() {
        return fail(format!("{} selects no skills", path.display()));
    }
    let mut selected = BTreeSet::new();
    for skill in &adoption.skills {
        if !SKILLS.contains(&skill.as_str()) {
            return fail(format!("unknown skill `{skill}` in {}", path.display()));
        }
        if !selected.insert(skill) {
            return fail(format!("duplicate skill `{skill}` in {}", path.display()));
        }
    }
    for (component_path, component) in &adoption.components {
        if !PROFILES.contains(&component.profile.as_str()) {
            return fail(format!(
                "unknown profile `{}` for component `{component_path}`",
                component.profile
            ));
        }
        let mut domains = BTreeSet::new();
        for domain in &component.domains {
            if !DOMAINS.contains(&domain.as_str()) {
                return fail(format!(
                    "unknown domain `{domain}` for component `{component_path}`"
                ));
            }
            if !domains.insert(domain) {
                return fail(format!(
                    "duplicate domain `{domain}` for component `{component_path}`"
                ));
            }
        }
    }
    Ok(())
}

fn expected_generated(root: &Path) -> TaskResult<BTreeMap<PathBuf, Vec<u8>>> {
    let mut output = BTreeMap::new();
    for skill in SKILLS {
        let source_root = root.join("skills").join(skill);
        for source in recursive_files(&source_root)? {
            let relative = source.strip_prefix(&source_root).map_err(|error| {
                task_error(format!(
                    "could not relativize {}: {error}",
                    source.display()
                ))
            })?;
            let bytes = fs::read(&source)?;
            output.insert(
                PathBuf::from("agent-skills").join(skill).join(relative),
                bytes.clone(),
            );
            if !relative.starts_with("agents") {
                output.insert(
                    PathBuf::from("claude-skills").join(skill).join(relative),
                    bytes.clone(),
                );
            }
            if relative == Path::new("agents/openai.yaml") {
                output.insert(
                    PathBuf::from("codex-overlay").join(skill).join(relative),
                    bytes,
                );
            }
        }
    }

    let manifest = generated_manifest(&output);
    output.insert(PathBuf::from("manifest.toml"), manifest.into_bytes());
    Ok(output)
}

fn generated_manifest(files: &BTreeMap<PathBuf, Vec<u8>>) -> String {
    let mut manifest =
        format!("schema = 1\nstandards = \"{VERSION}\"\nhash_algorithm = \"fnv1a64\"\n");
    for (path, bytes) in files {
        manifest.push_str("\n[[files]]\npath = \"");
        manifest.push_str(&portable_path(path));
        manifest.push_str("\"\nhash = \"");
        manifest.push_str(&format!("{:016x}", fnv1a64(bytes)));
        manifest.push_str("\"\n");
    }
    manifest
}

fn write_generated(root: &Path) -> TaskResult<()> {
    let expected = expected_generated(root)?;
    let target = root.join("generated");
    let temporary = root.join(format!("generated.rsl-tmp-{}", process::id()));
    let previous = root.join(format!("generated.rsl-old-{}", process::id()));

    remove_directory_if_present(&temporary)?;
    remove_directory_if_present(&previous)?;
    write_file_map(&temporary, &expected)?;

    if target.exists() {
        fs::rename(&target, &previous).map_err(|error| {
            task_error(format!(
                "could not stage existing generated directory {}: {error}",
                target.display()
            ))
        })?;
    }
    if let Err(error) = fs::rename(&temporary, &target) {
        if previous.exists() {
            let _restore_result = fs::rename(&previous, &target);
        }
        return fail(format!(
            "could not install generated directory {}: {error}",
            target.display()
        ));
    }
    remove_directory_if_present(&previous)?;
    Ok(())
}

fn check_generated(root: &Path) -> TaskResult<()> {
    let expected = expected_generated(root)?;
    let generated_root = root.join("generated");
    if !generated_root.is_dir() {
        return fail(format!(
            "{} is missing; run `cargo xtask generate`",
            generated_root.display()
        ));
    }
    let actual = read_file_map(&generated_root)?;
    let mut differences = Vec::new();

    for path in expected.keys() {
        match actual.get(path) {
            None => differences.push(format!("missing {}", portable_path(path))),
            Some(actual_bytes) if actual_bytes != &expected[path] => {
                differences.push(format!("changed {}", portable_path(path)));
            }
            Some(_) => {}
        }
    }
    for path in actual.keys() {
        if !expected.contains_key(path) {
            differences.push(format!("stale {}", portable_path(path)));
        }
    }
    if differences.is_empty() {
        Ok(())
    } else {
        fail(format!(
            "generated adapters drifted; run `cargo xtask generate`:\n- {}",
            differences.join("\n- ")
        ))
    }
}

fn report_adoption_files(repository: &Path, adoption: &Adoption) {
    let agents = repository.join("AGENTS.md");
    println!(
        "AGENTS.md: {}",
        if agents.is_file() {
            "present"
        } else {
            "missing"
        }
    );
    if repository.join(".rules").exists() {
        println!("warning: .rules may mask AGENTS.md in Zed");
    }
    if repository.join("CLAUDE.md").is_file() {
        println!("CLAUDE.md: present; verify it imports canonical AGENTS.md");
    } else {
        println!("CLAUDE.md: absent");
    }
    for skill in &adoption.skills {
        let common = repository.join(".agents/skills").join(skill).is_dir();
        let claude = repository.join(".claude/skills").join(skill).is_dir();
        println!("{skill}: common={common}, claude={claude}");
        if common && claude {
            println!(
                "warning: `{skill}` appears in both roots; Cursor coexistence remains unverified"
            );
        }
    }
}

fn install_base(scope: InstallScope, target: Option<PathBuf>) -> TaskResult<PathBuf> {
    let base = match target {
        Some(path) => path,
        None if scope == InstallScope::Repo => env::current_dir()?,
        None => PathBuf::from(
            env::var_os("HOME")
                .ok_or_else(|| task_error("user installation requires --target or HOME"))?,
        ),
    };
    canonical_directory(&base)
}

fn install_generated(
    root: &Path,
    base: &Path,
    agent: InstallAgent,
    scope: InstallScope,
    replace: bool,
) -> TaskResult<()> {
    let (source_name, destination_relative) = match agent {
        InstallAgent::Common => ("agent-skills", ".agents/skills"),
        InstallAgent::Claude => ("claude-skills", ".claude/skills"),
    };
    let source_root = root.join("generated").join(source_name);
    let destination_root = base.join(destination_relative);
    let staging = base.join(format!(".rsl-rust-install-{}", process::id()));
    remove_directory_if_present(&staging)?;
    fs::create_dir_all(&staging)?;

    for skill in SKILLS {
        let destination = destination_root.join(skill);
        if destination.exists() && !replace {
            remove_directory_if_present(&staging)?;
            return fail(format!(
                "refusing to overwrite {}; inspect it and pass --replace explicitly",
                destination.display()
            ));
        }
        copy_directory(&source_root.join(skill), &staging.join(skill))?;
    }

    fs::create_dir_all(&destination_root)?;
    for skill in SKILLS {
        let destination = destination_root.join(skill);
        if destination.exists() {
            remove_directory_if_present(&destination)?;
        }
        fs::rename(staging.join(skill), &destination)?;
        println!("installed {}", destination.display());
    }
    remove_directory_if_present(&staging)?;

    if agent == InstallAgent::Claude && scope == InstallScope::Repo {
        let claude_path = base.join("CLAUDE.md");
        if !claude_path.exists() {
            fs::copy(root.join("templates/CLAUDE.md.template"), &claude_path)?;
            println!("installed {}", claude_path.display());
        } else {
            println!(
                "left existing {} unchanged; verify it imports AGENTS.md",
                claude_path.display()
            );
        }
    }
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> TaskResult<()> {
    if !source.is_dir() {
        return fail(format!("missing source directory {}", source.display()));
    }
    fs::create_dir_all(destination)?;
    for entry in sorted_entries(source)? {
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), target)?;
        } else {
            return fail(format!(
                "unsupported non-file entry {}",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

fn write_file_map(root: &Path, files: &BTreeMap<PathBuf, Vec<u8>>) -> TaskResult<()> {
    for (relative, bytes) in files {
        let destination = root.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(destination, bytes)?;
    }
    Ok(())
}

fn read_file_map(root: &Path) -> TaskResult<BTreeMap<PathBuf, Vec<u8>>> {
    let mut files = BTreeMap::new();
    for path in recursive_files(root)? {
        let relative = path.strip_prefix(root).map_err(|error| {
            task_error(format!("could not relativize {}: {error}", path.display()))
        })?;
        files.insert(relative.to_path_buf(), fs::read(path)?);
    }
    Ok(files)
}

fn recursive_files(root: &Path) -> TaskResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files(root: &Path, files: &mut Vec<PathBuf>) -> TaskResult<()> {
    for entry in sorted_entries(root)? {
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files(&entry.path(), files)?;
        } else if file_type.is_file() {
            files.push(entry.path());
        } else {
            return fail(format!(
                "unsupported non-file entry {}",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

fn direct_directories(root: &Path) -> TaskResult<Vec<PathBuf>> {
    let mut directories = Vec::new();
    for entry in sorted_entries(root)? {
        if entry.file_type()?.is_dir() {
            directories.push(entry.path());
        } else {
            return fail(format!(
                "{} must contain only skill directories",
                root.display()
            ));
        }
    }
    Ok(directories)
}

fn direct_files(root: &Path) -> TaskResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in sorted_entries(root)? {
        if entry.file_type()?.is_file() {
            files.push(entry.path());
        }
    }
    Ok(files)
}

fn reject_nested_directories(root: &Path) -> TaskResult<()> {
    for entry in sorted_entries(root)? {
        if entry.file_type()?.is_dir() {
            return fail(format!(
                "references must remain one level deep: {}",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

fn sorted_entries(root: &Path) -> TaskResult<Vec<fs::DirEntry>> {
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(root)
        .map_err(|error| task_error(format!("could not read {}: {error}", root.display())))?
        .collect::<Result<_, _>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    Ok(entries)
}

fn remove_directory_if_present(path: &Path) -> TaskResult<()> {
    if path.exists() {
        if !path.is_dir() {
            return fail(format!(
                "refusing to remove non-directory {}",
                path.display()
            ));
        }
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn require_file(path: &Path) -> TaskResult<()> {
    if path.is_file() {
        Ok(())
    } else {
        fail(format!("missing required file {}", path.display()))
    }
}

fn read_text(path: &Path) -> TaskResult<String> {
    fs::read_to_string(path)
        .map_err(|error| task_error(format!("could not read {}: {error}", path.display())))
}

fn portable_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn task_error(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(TaskError(message.into()))
}

fn fail<T>(message: impl Into<String>) -> TaskResult<T> {
    Err(task_error(message))
}

#[cfg(test)]
mod tests {
    use super::{fnv1a64, parse_adoption, parse_frontmatter, project_root, validate_source};
    use std::path::Path;

    #[test]
    fn fnv_hash_is_stable() {
        assert_eq!(fnv1a64(b"hello"), 0xa430_d846_80aa_bd0b);
    }

    #[test]
    fn adoption_template_is_valid() -> Result<(), Box<dyn std::error::Error>> {
        let root = project_root()?;
        let path = root.join("templates/rsl-rust-standards.toml");
        let text = std::fs::read_to_string(&path)?;
        let adoption = parse_adoption(&text, &path)?;
        assert_eq!(adoption.schema, 1);
        assert_eq!(adoption.skills.len(), 2);
        assert!(adoption.components.is_empty());
        Ok(())
    }

    #[test]
    fn frontmatter_rejects_product_fields() {
        let result = parse_frontmatter(
            "---\nname: sample\ndescription: enough description for validation\nallowed-tools: shell\n---\n",
            Path::new("SKILL.md"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn repository_source_validates() -> Result<(), Box<dyn std::error::Error>> {
        validate_source(&project_root()?)
    }
}
