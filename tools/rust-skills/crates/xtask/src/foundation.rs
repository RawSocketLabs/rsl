use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

type ValidationResult<T> = Result<T, String>;

const CANONICAL_SKILLS: &[&str] = &[
    "rust-core",
    "rust-implement",
    "rust-review",
    "rust-repository-onboarding",
    "rust-api-design",
    "rust-testing",
    "rust-protocol",
    "rust-dsp",
    "rust-performance",
    "rust-async-concurrency",
    "rust-unsafe-ffi",
    "rust-dependencies-security",
    "rust-embedded",
    "rust-skill-maintenance",
];

const BASE_PROFILES: &[&str] = &[
    "public-library",
    "internal-library",
    "application",
    "service",
    "experimental",
];

const CAPABILITIES: &[&str] = &[
    "protocol",
    "parser-serializer",
    "dsp",
    "networking",
    "cryptography",
    "async",
    "concurrent",
    "real-time",
    "embedded",
    "no-std",
    "ffi",
    "platform-specific",
    "performance-sensitive",
    "security-sensitive",
    "safety-critical",
    "workspace",
    "public-api",
    "generated-code",
];

const PRESETS: &[&str] = &[
    "async-service",
    "cli",
    "firmware",
    "infrastructure",
    "research",
    "mixed-workspace",
];

const REFERENCE_TIERS: &[&str] = &[
    "authoritative",
    "approved-guidance",
    "curated-examples",
    "advisory",
    "historical",
];

#[derive(Debug)]
struct SkillManifest {
    path: PathBuf,
    id: String,
    package: String,
    status: String,
    kind: String,
    summary: String,
    requires: Vec<String>,
    routes_to: Vec<String>,
    related: Vec<String>,
    profiles: Vec<String>,
    packs: Vec<String>,
    references: Vec<String>,
    validation: Vec<String>,
    outputs: Vec<String>,
    legacy_ids: Vec<String>,
}

#[derive(Debug)]
struct ProfileManifest {
    path: PathBuf,
    id: String,
    kind: String,
    summary: String,
    base: String,
    capabilities: Vec<String>,
    skills: Vec<String>,
    implies: Vec<String>,
}

#[derive(Debug)]
struct PackManifest {
    id: String,
    summary: String,
    skills: Vec<String>,
}

#[derive(Debug)]
struct ReferenceManifest {
    path: PathBuf,
    id: String,
    tier: String,
    title: String,
    locator: String,
    revision: String,
    license: String,
    use_description: String,
    limitations: String,
    reviewed: String,
    refresh: String,
}

#[derive(Debug)]
struct MigrationManifest {
    id: String,
    from: String,
    to: String,
    status: String,
    introduced: String,
    behavior_change: String,
    install_action: String,
    retained: Vec<String>,
    changed: Vec<String>,
}

#[derive(Debug)]
struct ExampleManifest {
    path: PathBuf,
    id: String,
    title: String,
    skills: Vec<String>,
    profiles: Vec<String>,
    source: String,
    revision: String,
    provenance: String,
    validation: Vec<String>,
    document: String,
}

#[derive(Debug)]
struct Document {
    path: PathBuf,
    values: BTreeMap<String, String>,
}

impl Document {
    fn load(path: &Path) -> ValidationResult<Self> {
        let text = fs::read_to_string(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let mut values = BTreeMap::new();
        for (index, raw_line) in text.lines().enumerate() {
            let line = raw_line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                return Err(format!(
                    "sections are not supported in foundation manifest {}:{}",
                    path.display(),
                    index + 1
                ));
            }
            let (key, value) = line.split_once('=').ok_or_else(|| {
                format!(
                    "invalid foundation manifest line {}:{}",
                    path.display(),
                    index + 1
                )
            })?;
            let key = key.trim();
            let value = value.trim();
            if key.is_empty() || value.is_empty() {
                return Err(format!(
                    "empty key or value in {}:{}",
                    path.display(),
                    index + 1
                ));
            }
            if values.insert(key.to_owned(), value.to_owned()).is_some() {
                return Err(format!("duplicate key `{key}` in {}", path.display()));
            }
        }
        Ok(Self {
            path: path.to_path_buf(),
            values,
        })
    }

    fn schema(&mut self) -> ValidationResult<()> {
        let raw = self.take_raw("schema")?;
        let schema: u32 = raw
            .parse()
            .map_err(|error| format!("invalid schema in {}: {error}", self.path.display()))?;
        if schema != 1 {
            return Err(format!("{} must use schema 1", self.path.display()));
        }
        Ok(())
    }

    fn string(&mut self, key: &str) -> ValidationResult<String> {
        let raw = self.take_raw(key)?;
        parse_string(&raw, &self.path)
    }

    fn array(&mut self, key: &str) -> ValidationResult<Vec<String>> {
        let raw = self.take_raw(key)?;
        let values = parse_string_array(&raw, &self.path)?;
        ensure_unique(&values, key, &self.path)?;
        Ok(values)
    }

    fn finish(self) -> ValidationResult<()> {
        if self.values.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "unknown keys in {}: {:?}",
                self.path.display(),
                self.values.keys().collect::<Vec<_>>()
            ))
        }
    }

    fn take_raw(&mut self, key: &str) -> ValidationResult<String> {
        self.values
            .remove(key)
            .ok_or_else(|| format!("{} is missing `{key}`", self.path.display()))
    }
}

pub(crate) fn validate(root: &Path) -> ValidationResult<()> {
    validate_schema_contracts(root)?;
    let skills = load_skills(root)?;
    let routers = load_compatibility_skills(root, &skills)?;
    let profiles = load_profiles(root, &skills)?;
    let packs = load_packs(root, &skills)?;
    let references = load_references(root)?;
    let installable: Vec<&SkillManifest> = skills.values().chain(routers.values()).collect();
    validate_skill_metadata(&installable, &profiles, &packs, &references)?;
    validate_prose_routing(root, &installable)?;
    validate_migrations(root, &skills)?;
    validate_organization(root, &packs)?;
    validate_examples(root, &skills, &profiles, &references)?;
    Ok(())
}

fn validate_schema_contracts(root: &Path) -> ValidationResult<()> {
    for relative in [
        "catalog/schema/skill-manifest.toml",
        "catalog/schema/profile-manifest.toml",
        "catalog/schema/pack-manifest.toml",
        "catalog/schema/reference-manifest.toml",
        "catalog/schema/migration-manifest.toml",
        "catalog/schema/organization-decision.toml",
        "catalog/schema/curated-example.toml",
        "catalog/schema/evaluation-case.toml",
        "catalog/schema/onboarding-answers.toml",
        "catalog/schema/repository-adoption.toml",
        "catalog/schema/repository-validation.toml",
    ] {
        let path = root.join(relative);
        let mut document = Document::load(&path)?;
        document.schema()?;
        if document.values.is_empty() {
            return Err(format!("{relative} does not describe any fields"));
        }
    }
    Ok(())
}

fn load_skills(root: &Path) -> ValidationResult<BTreeMap<String, SkillManifest>> {
    let paths = load_index(root, "catalog/skills.toml")?;
    let mut skills = BTreeMap::new();
    for path in paths {
        let manifest = parse_skill_manifest(&path)?;
        validate_skill_shape(root, &manifest)?;
        let id = manifest.id.clone();
        if skills.insert(id.clone(), manifest).is_some() {
            return Err(format!("duplicate canonical skill ID `{id}`"));
        }
    }
    require_exact_ids(skills.keys(), CANONICAL_SKILLS, "canonical skills")?;

    let known: BTreeSet<String> = skills.keys().cloned().collect();
    let mut requires_graph = BTreeMap::new();
    for skill in skills.values() {
        validate_relation(&skill.id, "requires", &skill.requires, &known)?;
        validate_relation(&skill.id, "routes_to", &skill.routes_to, &known)?;
        validate_relation(&skill.id, "related", &skill.related, &known)?;
        requires_graph.insert(skill.id.clone(), skill.requires.clone());
    }
    validate_acyclic(&requires_graph, "skill `requires`")?;
    Ok(skills)
}

/// Loads the compatibility routers, which are installable but deliberately
/// outside the canonical catalog, its pack selections, and its default install.
fn load_compatibility_skills(
    root: &Path,
    canonical: &BTreeMap<String, SkillManifest>,
) -> ValidationResult<BTreeMap<String, SkillManifest>> {
    let paths = load_index(root, "catalog/compatibility-skills.toml")?;
    let mut routers = BTreeMap::new();
    for path in paths {
        let manifest = parse_skill_manifest(&path)?;
        validate_skill_shape(root, &manifest)?;
        if manifest.status != "compatibility" {
            return Err(format!(
                "indexed router `{}` declares status `{}`",
                manifest.id, manifest.status
            ));
        }
        if canonical.contains_key(&manifest.id) {
            return Err(format!(
                "compatibility router `{}` reuses a canonical skill ID",
                manifest.id
            ));
        }
        if !manifest.packs.is_empty() {
            return Err(format!(
                "compatibility router `{}` must not join an installable pack",
                manifest.id
            ));
        }
        if !canonical
            .values()
            .any(|skill| skill.legacy_ids.contains(&manifest.id))
        {
            return Err(format!(
                "compatibility router `{}` has no canonical successor claiming it as a legacy ID",
                manifest.id
            ));
        }
        let id = manifest.id.clone();
        if routers.insert(id.clone(), manifest).is_some() {
            return Err(format!("duplicate compatibility skill ID `{id}`"));
        }
    }

    let known: BTreeSet<String> = canonical.keys().chain(routers.keys()).cloned().collect();
    let mut requires_graph = BTreeMap::new();
    for skill in canonical.values().chain(routers.values()) {
        if routers.contains_key(&skill.id) {
            validate_relation(&skill.id, "requires", &skill.requires, &known)?;
            validate_relation(&skill.id, "routes_to", &skill.routes_to, &known)?;
            validate_relation(&skill.id, "related", &skill.related, &known)?;
        }
        requires_graph.insert(skill.id.clone(), skill.requires.clone());
    }
    validate_acyclic(&requires_graph, "skill `requires`")?;
    Ok(routers)
}

/// Rejects a package that routes to a skill in prose without declaring the
/// relation, which is how routing silently drifts away from the ownership graph.
fn validate_prose_routing(root: &Path, manifests: &[&SkillManifest]) -> ValidationResult<()> {
    let known: BTreeSet<&str> = manifests.iter().map(|skill| skill.id.as_str()).collect();
    for skill in manifests {
        let declared: BTreeSet<&str> = skill
            .requires
            .iter()
            .chain(&skill.routes_to)
            .chain(&skill.related)
            .map(String::as_str)
            .collect();
        for document in package_documents(&root.join("skills").join(&skill.package))? {
            let text = fs::read_to_string(&document)
                .map_err(|error| format!("could not read {}: {error}", document.display()))?;
            for mention in skill_mentions(&text) {
                if mention == skill.id {
                    continue;
                }
                if !known.contains(mention.as_str()) {
                    return Err(format!(
                        "{} mentions `${mention}`, which is not an installable skill",
                        document.display()
                    ));
                }
                if !declared.contains(mention.as_str()) {
                    return Err(format!(
                        "{} routes to `${mention}`, but `{}` does not declare it in `requires`, `routes_to`, or `related`",
                        document.display(),
                        skill.id
                    ));
                }
            }
        }
    }
    Ok(())
}

fn package_documents(package_root: &Path) -> ValidationResult<Vec<PathBuf>> {
    let mut documents = vec![package_root.join("SKILL.md")];
    let references = package_root.join("references");
    if references.is_dir() {
        let mut paths = Vec::new();
        let entries = fs::read_dir(&references)
            .map_err(|error| format!("could not read {}: {error}", references.display()))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| format!("could not read {}: {error}", references.display()))?;
            let path = entry.path();
            if path.is_file() {
                paths.push(path);
            }
        }
        paths.sort();
        documents.extend(paths);
    }
    Ok(documents)
}

/// Collects `$skill` activation sigils. Only the two skill namespaces are
/// recognized so ordinary shell or currency text is not mistaken for routing.
fn skill_mentions(text: &str) -> BTreeSet<String> {
    let mut mentions = BTreeSet::new();
    let mut remaining = text;
    while let Some(offset) = remaining.find('$') {
        remaining = &remaining[offset + 1..];
        let end = remaining
            .find(|character: char| {
                !(character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
            })
            .unwrap_or(remaining.len());
        let token = remaining[..end].trim_end_matches('-');
        if token.starts_with("rust-") || token.starts_with("rsl-rust-") {
            mentions.insert(token.to_owned());
        }
        remaining = &remaining[end..];
    }
    mentions
}

fn parse_skill_manifest(path: &Path) -> ValidationResult<SkillManifest> {
    let mut document = Document::load(path)?;
    document.schema()?;
    let manifest = SkillManifest {
        path: path.to_path_buf(),
        id: document.string("id")?,
        package: document.string("package")?,
        status: document.string("status")?,
        kind: document.string("kind")?,
        summary: document.string("summary")?,
        requires: document.array("requires")?,
        routes_to: document.array("routes_to")?,
        related: document.array("related")?,
        profiles: document.array("profiles")?,
        packs: document.array("packs")?,
        references: document.array("references")?,
        validation: document.array("validation")?,
        outputs: document.array("outputs")?,
        legacy_ids: document.array("legacy_ids")?,
    };
    document.finish()?;
    Ok(manifest)
}

fn validate_skill_shape(root: &Path, skill: &SkillManifest) -> ValidationResult<()> {
    if !valid_id(&skill.id) {
        return Err(format!("invalid canonical skill ID `{}`", skill.id));
    }
    if !matches!(
        skill.status.as_str(),
        "implemented" | "compatibility" | "planned"
    ) {
        return Err(format!(
            "unknown status `{}` for skill `{}`",
            skill.status, skill.id
        ));
    }
    if !matches!(
        skill.kind.as_str(),
        "foundation" | "task" | "domain" | "technique" | "environment" | "maintenance"
    ) {
        return Err(format!(
            "unknown kind `{}` for skill `{}`",
            skill.kind, skill.id
        ));
    }
    require_nonempty(&skill.summary, "summary", &skill.path)?;
    require_nonempty_array(&skill.profiles, "profiles", &skill.path)?;
    require_nonempty_array(&skill.references, "references", &skill.path)?;
    require_nonempty_array(&skill.validation, "validation", &skill.path)?;
    require_nonempty_array(&skill.outputs, "outputs", &skill.path)?;
    for legacy in &skill.legacy_ids {
        if !valid_id(legacy) {
            return Err(format!("invalid legacy skill ID `{legacy}`"));
        }
    }

    match skill.status.as_str() {
        "planned" => {
            if !skill.package.is_empty() {
                return Err(format!(
                    "planned skill `{}` must not declare an installable package",
                    skill.id
                ));
            }
            let planned_root = root.join("catalog/planned");
            if !skill.path.starts_with(planned_root) {
                return Err(format!(
                    "planned skill `{}` must live under catalog/planned",
                    skill.id
                ));
            }
        }
        "implemented" => {
            if skill.package != skill.id {
                return Err(format!(
                    "implemented skill `{}` must use the canonical package name",
                    skill.id
                ));
            }
            require_package_manifest_path(root, skill)?;
        }
        "compatibility" => {
            if !valid_id(&skill.package) || !skill.legacy_ids.contains(&skill.package) {
                return Err(format!(
                    "compatibility skill `{}` must declare its package as a legacy ID",
                    skill.id
                ));
            }
            require_package_manifest_path(root, skill)?;
        }
        _ => {}
    }
    Ok(())
}

fn require_package_manifest_path(root: &Path, skill: &SkillManifest) -> ValidationResult<()> {
    let expected = root.join("skills").join(&skill.package).join("skill.toml");
    if skill.path != expected {
        return Err(format!(
            "skill `{}` manifest must be at {}, found {}",
            skill.id,
            expected.display(),
            skill.path.display()
        ));
    }
    Ok(())
}

fn validate_relation(
    owner: &str,
    relation: &str,
    targets: &[String],
    known: &BTreeSet<String>,
) -> ValidationResult<()> {
    for target in targets {
        if target == owner {
            return Err(format!("skill `{owner}` has a self `{relation}` relation"));
        }
        if !known.contains(target) {
            return Err(format!(
                "skill `{owner}` has unknown `{relation}` target `{target}`"
            ));
        }
    }
    Ok(())
}

fn load_profiles(
    root: &Path,
    skills: &BTreeMap<String, SkillManifest>,
) -> ValidationResult<BTreeMap<String, ProfileManifest>> {
    let paths = load_index(root, "profiles/index.toml")?;
    let known_skills: BTreeSet<String> = skills.keys().cloned().collect();
    let mut profiles = BTreeMap::new();
    for path in paths {
        let profile = parse_profile_manifest(&path)?;
        if !valid_id(&profile.id) {
            return Err(format!("invalid profile ID `{}`", profile.id));
        }
        require_nonempty(&profile.summary, "summary", &profile.path)?;
        validate_known_items(&profile.skills, &known_skills, "skill", &profile.id)?;
        validate_profile_path(root, &profile)?;
        let id = profile.id.clone();
        if profiles.insert(id.clone(), profile).is_some() {
            return Err(format!("duplicate profile ID `{id}`"));
        }
    }

    let base_ids = profile_ids_of_kind(&profiles, "base");
    let capability_ids = profile_ids_of_kind(&profiles, "capability");
    let preset_ids = profile_ids_of_kind(&profiles, "preset");
    require_exact_ids(base_ids.iter(), BASE_PROFILES, "base profiles")?;
    require_exact_ids(capability_ids.iter(), CAPABILITIES, "capabilities")?;
    require_exact_ids(preset_ids.iter(), PRESETS, "presets")?;

    let mut implication_graph = BTreeMap::new();
    for profile in profiles.values() {
        match profile.kind.as_str() {
            "base" => {
                require_empty(&profile.base, "base", &profile.path)?;
                require_empty_array(&profile.capabilities, "capabilities", &profile.path)?;
                require_empty_array(&profile.implies, "implies", &profile.path)?;
            }
            "capability" => {
                require_empty(&profile.base, "base", &profile.path)?;
                require_empty_array(&profile.capabilities, "capabilities", &profile.path)?;
                validate_known_items(
                    &profile.implies,
                    &capability_ids,
                    "implied capability",
                    &profile.id,
                )?;
                implication_graph.insert(profile.id.clone(), profile.implies.clone());
            }
            "preset" => {
                if !base_ids.contains(&profile.base) {
                    return Err(format!(
                        "preset `{}` has unknown base `{}`",
                        profile.id, profile.base
                    ));
                }
                validate_known_items(
                    &profile.capabilities,
                    &capability_ids,
                    "capability",
                    &profile.id,
                )?;
                require_empty_array(&profile.skills, "skills", &profile.path)?;
                require_empty_array(&profile.implies, "implies", &profile.path)?;
            }
            other => {
                return Err(format!(
                    "unknown profile kind `{other}` in {}",
                    profile.path.display()
                ));
            }
        }
    }
    validate_acyclic(&implication_graph, "capability `implies`")?;
    Ok(profiles)
}

fn parse_profile_manifest(path: &Path) -> ValidationResult<ProfileManifest> {
    let mut document = Document::load(path)?;
    document.schema()?;
    let profile = ProfileManifest {
        path: path.to_path_buf(),
        id: document.string("id")?,
        kind: document.string("kind")?,
        summary: document.string("summary")?,
        base: document.string("base")?,
        capabilities: document.array("capabilities")?,
        skills: document.array("skills")?,
        implies: document.array("implies")?,
    };
    document.finish()?;
    Ok(profile)
}

fn validate_profile_path(root: &Path, profile: &ProfileManifest) -> ValidationResult<()> {
    let expected_parent = root.join("profiles").join(match profile.kind.as_str() {
        "base" => "base",
        "capability" => "capabilities",
        "preset" => "presets",
        _ => return Ok(()),
    });
    if profile.path.parent() != Some(expected_parent.as_path()) {
        return Err(format!(
            "profile `{}` must live under {}",
            profile.id,
            expected_parent.display()
        ));
    }
    Ok(())
}

fn profile_ids_of_kind(
    profiles: &BTreeMap<String, ProfileManifest>,
    kind: &str,
) -> BTreeSet<String> {
    profiles
        .values()
        .filter(|profile| profile.kind == kind)
        .map(|profile| profile.id.clone())
        .collect()
}

fn load_packs(
    root: &Path,
    skills: &BTreeMap<String, SkillManifest>,
) -> ValidationResult<BTreeMap<String, PackManifest>> {
    let paths = load_index(root, "packs/index.toml")?;
    let known_skills: BTreeSet<String> = skills.keys().cloned().collect();
    let mut packs = BTreeMap::new();
    for path in paths {
        let mut document = Document::load(&path)?;
        document.schema()?;
        let pack = PackManifest {
            id: document.string("id")?,
            summary: document.string("summary")?,
            skills: document.array("skills")?,
        };
        document.finish()?;
        if !valid_id(&pack.id) {
            return Err(format!("invalid pack ID `{}`", pack.id));
        }
        require_nonempty(&pack.summary, "summary", &path)?;
        require_nonempty_array(&pack.skills, "skills", &path)?;
        validate_known_items(&pack.skills, &known_skills, "skill", &pack.id)?;
        let id = pack.id.clone();
        if packs.insert(id.clone(), pack).is_some() {
            return Err(format!("duplicate pack ID `{id}`"));
        }
    }
    Ok(packs)
}

fn load_references(root: &Path) -> ValidationResult<BTreeMap<String, ReferenceManifest>> {
    let paths = load_index(root, "references/index.toml")?;
    let mut references = BTreeMap::new();
    for path in paths {
        let reference = parse_reference_manifest(&path)?;
        validate_reference_shape(root, &reference)?;
        let id = reference.id.clone();
        if references.insert(id.clone(), reference).is_some() {
            return Err(format!("duplicate reference ID `{id}`"));
        }
    }
    Ok(references)
}

fn parse_reference_manifest(path: &Path) -> ValidationResult<ReferenceManifest> {
    let mut document = Document::load(path)?;
    document.schema()?;
    let reference = ReferenceManifest {
        path: path.to_path_buf(),
        id: document.string("id")?,
        tier: document.string("tier")?,
        title: document.string("title")?,
        locator: document.string("locator")?,
        revision: document.string("revision")?,
        license: document.string("license")?,
        use_description: document.string("use")?,
        limitations: document.string("limitations")?,
        reviewed: document.string("reviewed")?,
        refresh: document.string("refresh")?,
    };
    document.finish()?;
    Ok(reference)
}

fn validate_reference_shape(root: &Path, reference: &ReferenceManifest) -> ValidationResult<()> {
    if !valid_id(&reference.id) {
        return Err(format!("invalid reference ID `{}`", reference.id));
    }
    if !REFERENCE_TIERS.contains(&reference.tier.as_str()) {
        return Err(format!(
            "unknown reference tier `{}` for `{}`",
            reference.tier, reference.id
        ));
    }
    let expected_parent = root.join("references").join(&reference.tier);
    if reference.path.parent() != Some(expected_parent.as_path()) {
        return Err(format!(
            "reference `{}` must live under {}",
            reference.id,
            expected_parent.display()
        ));
    }
    for (field, value) in [
        ("title", &reference.title),
        ("locator", &reference.locator),
        ("revision", &reference.revision),
        ("license", &reference.license),
        ("use", &reference.use_description),
        ("limitations", &reference.limitations),
        ("reviewed", &reference.reviewed),
        ("refresh", &reference.refresh),
    ] {
        require_nonempty(value, field, &reference.path)?;
    }
    Ok(())
}

fn validate_skill_metadata(
    skills: &[&SkillManifest],
    profiles: &BTreeMap<String, ProfileManifest>,
    packs: &BTreeMap<String, PackManifest>,
    references: &BTreeMap<String, ReferenceManifest>,
) -> ValidationResult<()> {
    let known_profiles: BTreeSet<String> = profiles.keys().cloned().collect();
    let known_packs: BTreeSet<String> = packs.keys().cloned().collect();
    let known_references: BTreeSet<String> = references.keys().cloned().collect();
    for skill in skills {
        for profile in &skill.profiles {
            if !matches!(profile.as_str(), "all" | "skills-repository")
                && !known_profiles.contains(profile)
            {
                return Err(format!(
                    "skill `{}` names unknown profile `{profile}`",
                    skill.id
                ));
            }
        }
        validate_known_items(&skill.packs, &known_packs, "pack", &skill.id)?;
        validate_known_items(&skill.references, &known_references, "reference", &skill.id)?;
        for pack in &skill.packs {
            if !packs[pack].skills.contains(&skill.id) {
                return Err(format!(
                    "skill `{}` declares pack `{pack}`, but the pack does not select it",
                    skill.id
                ));
            }
        }
    }
    Ok(())
}

fn validate_migrations(
    root: &Path,
    skills: &BTreeMap<String, SkillManifest>,
) -> ValidationResult<()> {
    let paths = load_index(root, "compatibility/index.toml")?;
    let mut migrations = BTreeMap::new();
    let legacy_targets: BTreeMap<String, String> = skills
        .values()
        .flat_map(|skill| {
            skill
                .legacy_ids
                .iter()
                .map(|legacy| (legacy.clone(), skill.id.clone()))
        })
        .collect();
    for path in paths {
        let migration = parse_migration_manifest(&path)?;
        if !valid_id(&migration.id) {
            return Err(format!("invalid migration ID `{}`", migration.id));
        }
        if !matches!(
            migration.status.as_str(),
            "planned" | "available" | "retired"
        ) {
            return Err(format!(
                "unknown migration status `{}` for `{}`",
                migration.status, migration.id
            ));
        }
        let Some(expected_target) = legacy_targets.get(&migration.from) else {
            return Err(format!(
                "migration `{}` has unknown legacy source `{}`",
                migration.id, migration.from
            ));
        };
        if expected_target != &migration.to {
            return Err(format!(
                "migration `{}` targets `{}`, expected `{expected_target}`",
                migration.id, migration.to
            ));
        }
        for (field, value) in [
            ("introduced", &migration.introduced),
            ("behavior_change", &migration.behavior_change),
            ("install_action", &migration.install_action),
        ] {
            require_nonempty(value, field, &path)?;
        }
        require_nonempty_array(&migration.retained, "retained", &path)?;
        require_nonempty_array(&migration.changed, "changed", &path)?;
        let from = migration.from.clone();
        if migrations.insert(from.clone(), migration).is_some() {
            return Err(format!("duplicate migration from `{from}`"));
        }
    }
    let actual: BTreeSet<String> = migrations.keys().cloned().collect();
    let expected: BTreeSet<String> = legacy_targets.keys().cloned().collect();
    if actual != expected {
        return Err(format!(
            "migration coverage mismatch: expected {expected:?}, found {actual:?}"
        ));
    }
    Ok(())
}

fn parse_migration_manifest(path: &Path) -> ValidationResult<MigrationManifest> {
    let mut document = Document::load(path)?;
    document.schema()?;
    let migration = MigrationManifest {
        id: document.string("id")?,
        from: document.string("from")?,
        to: document.string("to")?,
        status: document.string("status")?,
        introduced: document.string("introduced")?,
        behavior_change: document.string("behavior_change")?,
        install_action: document.string("install_action")?,
        retained: document.array("retained")?,
        changed: document.array("changed")?,
    };
    document.finish()?;
    Ok(migration)
}

fn validate_organization(
    root: &Path,
    packs: &BTreeMap<String, PackManifest>,
) -> ValidationResult<()> {
    let path = root.join("organizations/rsl/organization.toml");
    let mut document = Document::load(&path)?;
    document.schema()?;
    let id = document.string("id")?;
    let name = document.string("name")?;
    let default_pack = document.string("default_pack")?;
    let portable_prefix = document.string("portable_skill_prefix")?;
    let compatibility_prefix = document.string("compatibility_skill_prefix")?;
    let local_rule_strategy = document.string("local_rule_strategy")?;
    let precedence = document.string("precedence")?;
    let source = document.string("source")?;
    let decisions = document.array("decisions")?;
    document.finish()?;
    if id != "rsl" || name != "Raw Socket Labs" {
        return Err("RSL organization manifest has an unexpected identity".to_owned());
    }
    if !packs.contains_key(&default_pack) {
        return Err(format!("RSL default pack `{default_pack}` is unknown"));
    }
    if portable_prefix != "rust-" || compatibility_prefix != "rsl-rust-" {
        return Err("RSL organization manifest has unexpected skill prefixes".to_owned());
    }
    if local_rule_strategy != "preserve-and-index" {
        return Err("RSL local rule strategy must be `preserve-and-index`".to_owned());
    }
    if precedence != "repository-over-organization" {
        return Err(
            "RSL precedence must keep repository policy above organization policy".to_owned(),
        );
    }
    require_nonempty(&source, "source", &path)?;
    require_nonempty_array(&decisions, "decisions", &path)?;

    let decision_root = root.join("organizations/rsl/decisions");
    let mut ids = BTreeSet::new();
    for relative in decisions {
        validate_relative_path(&relative, &path)?;
        let decision_path = root.join(&relative);
        if decision_path.parent() != Some(decision_root.as_path()) {
            return Err(format!(
                "RSL decision `{relative}` must live directly under {}",
                decision_root.display()
            ));
        }
        let mut decision = Document::load(&decision_path)?;
        decision.schema()?;
        let decision_id = decision.string("id")?;
        let title = decision.string("title")?;
        let strength = decision.string("strength")?;
        let scope = decision.string("scope")?;
        let rule = decision.string("rule")?;
        let applies_when = decision.array("applies_when")?;
        let exceptions = decision.array("exceptions")?;
        let source_rules = decision.array("source_rules")?;
        decision.finish()?;

        if !valid_id(&decision_id) {
            return Err(format!("invalid RSL decision ID `{decision_id}`"));
        }
        if !ids.insert(decision_id.clone()) {
            return Err(format!("duplicate RSL decision ID `{decision_id}`"));
        }
        if !matches!(
            strength.as_str(),
            "MUST" | "MUST-NOT" | "SHOULD" | "SHOULD-NOT" | "PREFER" | "CONSIDER" | "MAY"
        ) {
            return Err(format!(
                "unknown RSL decision strength `{strength}` for `{decision_id}`"
            ));
        }
        for (field, value) in [("title", &title), ("scope", &scope), ("rule", &rule)] {
            require_nonempty(value, field, &decision_path)?;
        }
        require_nonempty_array(&applies_when, "applies_when", &decision_path)?;
        require_nonempty_array(&exceptions, "exceptions", &decision_path)?;
        require_nonempty_array(&source_rules, "source_rules", &decision_path)?;
        for source_rule in source_rules {
            if !valid_preference_rule(&source_rule) {
                return Err(format!(
                    "invalid preference rule `{source_rule}` in {}",
                    decision_path.display()
                ));
            }
        }
    }
    Ok(())
}

fn validate_examples(
    root: &Path,
    skills: &BTreeMap<String, SkillManifest>,
    profiles: &BTreeMap<String, ProfileManifest>,
    references: &BTreeMap<String, ReferenceManifest>,
) -> ValidationResult<()> {
    let path = root.join("examples/schema.toml");
    let mut document = Document::load(&path)?;
    document.schema()?;
    let metadata = document.array("metadata_fields")?;
    let sections = document.array("required_sections")?;
    let source_policy = document.string("source_policy")?;
    document.finish()?;
    for field in [
        "id",
        "title",
        "skills",
        "profiles",
        "source",
        "revision",
        "provenance",
        "validation",
        "document",
    ] {
        if !metadata.iter().any(|value| value == field) {
            return Err(format!(
                "example schema is missing metadata field `{field}`"
            ));
        }
    }
    for section in [
        "before",
        "review",
        "after",
        "tests",
        "lesson",
        "applies-when",
        "does-not-apply-when",
    ] {
        if !sections.iter().any(|value| value == section) {
            return Err(format!("example schema is missing section `{section}`"));
        }
    }
    require_nonempty(&source_policy, "source_policy", &path)?;

    let paths = load_index(root, "examples/index.toml")?;
    let known_skills: BTreeSet<String> = skills.keys().cloned().collect();
    let known_profiles: BTreeSet<String> = profiles.keys().cloned().collect();
    let known_references: BTreeSet<String> = references.keys().cloned().collect();
    let examples_root = root.join("examples");
    let mut ids = BTreeSet::new();

    for example_path in paths {
        if !example_path.starts_with(&examples_root)
            || example_path.file_name().and_then(|name| name.to_str()) != Some("example.toml")
        {
            return Err(format!(
                "example manifest {} must be an example.toml beneath {}",
                example_path.display(),
                examples_root.display()
            ));
        }
        let example = parse_example_manifest(&example_path)?;
        if !valid_id(&example.id) {
            return Err(format!("invalid example ID `{}`", example.id));
        }
        if !ids.insert(example.id.clone()) {
            return Err(format!("duplicate example ID `{}`", example.id));
        }
        require_nonempty(&example.title, "title", &example.path)?;
        require_nonempty_array(&example.skills, "skills", &example.path)?;
        require_nonempty_array(&example.profiles, "profiles", &example.path)?;
        require_nonempty(&example.source, "source", &example.path)?;
        require_nonempty(&example.revision, "revision", &example.path)?;
        require_nonempty(&example.provenance, "provenance", &example.path)?;
        require_nonempty_array(&example.validation, "validation", &example.path)?;
        validate_known_items(&example.skills, &known_skills, "skill", &example.id)?;
        for profile in &example.profiles {
            if profile != "all" && !known_profiles.contains(profile) {
                return Err(format!(
                    "example `{}` references unknown profile `{profile}`",
                    example.id
                ));
            }
        }
        if !known_references.contains(&example.source) {
            return Err(format!(
                "example `{}` references unknown source `{}`",
                example.id, example.source
            ));
        }

        validate_relative_path(&example.document, &example.path)?;
        let example_root = example.path.parent().ok_or_else(|| {
            format!(
                "example manifest {} has no parent directory",
                example.path.display()
            )
        })?;
        let example_document = example_root.join(&example.document);
        let content = fs::read_to_string(&example_document).map_err(|error| {
            format!(
                "could not read example document {}: {error}",
                example_document.display()
            )
        })?;
        for section in &sections {
            let heading = example_heading(section);
            if !content.lines().any(|line| line.trim() == heading) {
                return Err(format!(
                    "{} is missing required heading `{heading}`",
                    example_document.display()
                ));
            }
        }
    }
    Ok(())
}

fn parse_example_manifest(path: &Path) -> ValidationResult<ExampleManifest> {
    let mut document = Document::load(path)?;
    document.schema()?;
    let example = ExampleManifest {
        path: path.to_path_buf(),
        id: document.string("id")?,
        title: document.string("title")?,
        skills: document.array("skills")?,
        profiles: document.array("profiles")?,
        source: document.string("source")?,
        revision: document.string("revision")?,
        provenance: document.string("provenance")?,
        validation: document.array("validation")?,
        document: document.string("document")?,
    };
    document.finish()?;
    Ok(example)
}

fn example_heading(section: &str) -> &'static str {
    match section {
        "before" => "## Before",
        "review" => "## Review",
        "after" => "## After",
        "tests" => "## Tests",
        "lesson" => "## Lesson",
        "applies-when" => "## Applies when",
        "does-not-apply-when" => "## Does not apply when",
        _ => "",
    }
}

fn valid_preference_rule(value: &str) -> bool {
    value.strip_prefix('R').is_some_and(|number| {
        !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn load_index(root: &Path, relative: &str) -> ValidationResult<Vec<PathBuf>> {
    let path = root.join(relative);
    let mut document = Document::load(&path)?;
    document.schema()?;
    let manifests = document.array("manifests")?;
    document.finish()?;
    require_nonempty_array(&manifests, "manifests", &path)?;
    manifests
        .into_iter()
        .map(|manifest| {
            validate_relative_path(&manifest, &path)?;
            let target = root.join(&manifest);
            if !target.is_file() {
                return Err(format!(
                    "index {} references missing file {}",
                    path.display(),
                    target.display()
                ));
            }
            Ok(target)
        })
        .collect()
}

fn validate_relative_path(value: &str, owner: &Path) -> ValidationResult<()> {
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "path `{value}` escapes the repository in {}",
            owner.display()
        ));
    }
    Ok(())
}

fn parse_string(value: &str, path: &Path) -> ValidationResult<String> {
    if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
        return Err(format!(
            "expected quoted string in {}: {value}",
            path.display()
        ));
    }
    let inner = &value[1..value.len() - 1];
    if inner.contains('"') || inner.contains('\\') {
        return Err(format!(
            "escaped strings are not supported in {}",
            path.display()
        ));
    }
    Ok(inner.to_owned())
}

fn parse_string_array(value: &str, path: &Path) -> ValidationResult<Vec<String>> {
    if !value.starts_with('[') || !value.ends_with(']') {
        return Err(format!(
            "expected string array in {}: {value}",
            path.display()
        ));
    }
    let inner = value[1..value.len() - 1].trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    let mut raw_items = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    for (index, character) in inner.char_indices() {
        match character {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                raw_items.push(inner[start..index].trim());
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    if quoted {
        return Err(format!("unclosed quoted string in {}", path.display()));
    }
    raw_items.push(inner[start..].trim());
    raw_items
        .into_iter()
        .map(|item| parse_string(item, path))
        .collect()
}

fn ensure_unique(values: &[String], field: &str, path: &Path) -> ValidationResult<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(format!(
                "duplicate `{value}` in `{field}` in {}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn require_exact_ids<'a>(
    actual: impl Iterator<Item = &'a String>,
    expected: &[&str],
    label: &str,
) -> ValidationResult<()> {
    let actual: BTreeSet<&str> = actual.map(String::as_str).collect();
    let expected: BTreeSet<&str> = expected.iter().copied().collect();
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{label} mismatch: expected {expected:?}, found {actual:?}"
        ))
    }
}

fn validate_known_items(
    items: &[String],
    known: &BTreeSet<String>,
    item_kind: &str,
    owner: &str,
) -> ValidationResult<()> {
    for item in items {
        if !known.contains(item) {
            return Err(format!("`{owner}` references unknown {item_kind} `{item}`"));
        }
    }
    Ok(())
}

fn require_nonempty(value: &str, field: &str, path: &Path) -> ValidationResult<()> {
    if value.trim().is_empty() {
        Err(format!("`{field}` is empty in {}", path.display()))
    } else {
        Ok(())
    }
}

fn require_empty(value: &str, field: &str, path: &Path) -> ValidationResult<()> {
    if value.is_empty() {
        Ok(())
    } else {
        Err(format!("`{field}` must be empty in {}", path.display()))
    }
}

fn require_nonempty_array(values: &[String], field: &str, path: &Path) -> ValidationResult<()> {
    if values.is_empty() {
        Err(format!("`{field}` is empty in {}", path.display()))
    } else {
        Ok(())
    }
}

fn require_empty_array(values: &[String], field: &str, path: &Path) -> ValidationResult<()> {
    if values.is_empty() {
        Ok(())
    } else {
        Err(format!("`{field}` must be empty in {}", path.display()))
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn validate_acyclic(graph: &BTreeMap<String, Vec<String>>, relation: &str) -> ValidationResult<()> {
    let mut visited = BTreeSet::new();
    let mut visiting = BTreeSet::new();
    let mut path = Vec::new();
    for node in graph.keys() {
        visit(
            node,
            graph,
            relation,
            &mut visited,
            &mut visiting,
            &mut path,
        )?;
    }
    Ok(())
}

fn visit(
    node: &str,
    graph: &BTreeMap<String, Vec<String>>,
    relation: &str,
    visited: &mut BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
    path: &mut Vec<String>,
) -> ValidationResult<()> {
    if visited.contains(node) {
        return Ok(());
    }
    if !visiting.insert(node.to_owned()) {
        path.push(node.to_owned());
        return Err(format!("{relation} cycle: {}", path.join(" -> ")));
    }
    path.push(node.to_owned());
    if let Some(neighbors) = graph.get(node) {
        for neighbor in neighbors {
            visit(neighbor, graph, relation, visited, visiting, path)?;
        }
    }
    let removed = path.pop();
    if removed.as_deref() != Some(node) {
        return Err(format!("internal {relation} traversal error"));
    }
    visiting.remove(node);
    visited.insert(node.to_owned());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_string_array, skill_mentions, validate_acyclic};
    use std::collections::BTreeMap;
    use std::path::Path;

    #[test]
    fn skill_mentions_collect_both_namespaces() {
        let mentions = skill_mentions("Route to `$rust-testing` and `$rsl-rust-core`.");
        assert!(mentions.contains("rust-testing"));
        assert!(mentions.contains("rsl-rust-core"));
        assert_eq!(mentions.len(), 2);
    }

    #[test]
    fn skill_mentions_ignore_unrelated_sigils() {
        let mentions = skill_mentions("Set $PATH, spend $5, and export $cargo_home.");
        assert!(mentions.is_empty());
    }

    #[test]
    fn skill_mentions_stop_at_trailing_punctuation() {
        let mentions = skill_mentions("Prefer $rust-dsp, then $rust-protocol.");
        assert!(mentions.contains("rust-dsp"));
        assert!(mentions.contains("rust-protocol"));
        assert_eq!(mentions.len(), 2);
    }

    #[test]
    fn string_array_parser_rejects_unquoted_values() {
        let result = parse_string_array("[\"one\", two]", Path::new("manifest.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn graph_validator_rejects_cycles() {
        let graph = BTreeMap::from([
            ("one".to_owned(), vec!["two".to_owned()]),
            ("two".to_owned(), vec!["one".to_owned()]),
        ]);
        assert!(validate_acyclic(&graph, "test").is_err());
    }
}
