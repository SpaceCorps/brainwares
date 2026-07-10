use crate::engine::{check_vault_status, ReferenceStatus};
use crate::hash::calculate_file_hash;
use crate::models::{CodeReference, Frontmatter, MemoryPage};
use crate::parser::{parse_memory_file, serialize_memory_file};
use crate::vault::{get_backlinks, get_workspace_root, init_vault, load_memories};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};

// Helper to resolve memory name or path to the exact path of the memory file
fn resolve_memory_path(vault_path: &Path, input: &str) -> Result<PathBuf, String> {
    // 1. Check if input is already a path pointing to a file
    let path = PathBuf::from(input);
    if path.is_file() {
        return Ok(path);
    }

    let mut file_name = input.to_string();
    if !file_name.ends_with(".md") {
        file_name.push_str(".md");
    }

    // 2. Otherwise look up in local memories dir
    let local_memories_dir = vault_path.join("memories");
    let resolved = local_memories_dir.join(&file_name);
    if resolved.is_file() {
        return Ok(resolved);
    }

    // Try lowercased lookup in local memories dir
    if let Ok(entries) = fs::read_dir(&local_memories_dir) {
        let input_lower = file_name.to_lowercase();
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() && p.file_name().and_then(|n| n.to_str()).map(|s| s.to_lowercase()) == Some(input_lower.clone()) {
                return Ok(p);
            }
        }
    }

    // 3. Try global memories dir
    if let Some(global_config_path) = crate::vault::get_global_config_path() {
        if let Some(global_parent) = global_config_path.parent() {
            let global_memories_dir = global_parent.join("memories");
            let resolved_global = global_memories_dir.join(&file_name);
            if resolved_global.is_file() {
                return Ok(resolved_global);
            }

            // Lowercased lookup in global memories dir
            if let Ok(entries) = fs::read_dir(&global_memories_dir) {
                let input_lower = file_name.to_lowercase();
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() && p.file_name().and_then(|n| n.to_str()).map(|s| s.to_lowercase()) == Some(input_lower.clone()) {
                        return Ok(p);
                    }
                }
            }
        }
    }

    Err(format!(
        "Memory file '{}' not found in local memories directory {:?} or global vault memories.",
        input, local_memories_dir
    ))
}

pub fn handle_init(vault_path: &Path) -> Result<(), String> {
    println!("Initializing brainwares vault at {:?}", vault_path);
    let _config = init_vault(vault_path)?;
    println!("SUCCESS: Vault initialized successfully.");
    println!("Directory structure created:");
    println!("  - memories/ (Obsidian-style notes)");
    println!("  - programs/ (Promptware instruction programs)");
    println!("  - logs/     (Execution history)");
    println!("  - config.json");
    Ok(())
}

pub fn handle_status(vault_path: &Path) -> Result<(), String> {
    let memories = load_memories(vault_path)?;
    let status = check_vault_status(vault_path, &memories);

    println!("================= VAULT STATUS =================");
    println!("Vault path: {:?}", vault_path);
    println!("Total memories: {}", status.total_memories);
    println!("------------------------------------------------");

    for m in &status.memories {
        let mut issues = Vec::new();

        for ref_status in &m.references {
            match &ref_status.status {
                ReferenceStatus::Ok => {}
                ReferenceStatus::Outdated { stored, current } => {
                    issues.push(format!(
                        "  [OUTDATED CODE] {} (stored: {}, current: {})",
                        ref_status.path,
                        &stored[..std::cmp::min(8, stored.len())],
                        &current[..std::cmp::min(8, current.len())]
                    ));
                }
                ReferenceStatus::Missing => {
                    issues.push(format!("  [MISSING CODE] {}", ref_status.path));
                }
            }
        }

        for broken in &m.broken_links {
            issues.push(format!("  [BROKEN LINK] [[{}]]", broken));
        }

        if m.is_orphan {
            issues.push("  [ORPHAN] Not linked by any other memory page".to_string());
        }

        if m.has_placeholders {
            issues.push("  [INCOMPLETE] Contains pending file descriptions ([Enter description...])".to_string());
        }

        if !issues.is_empty() {
            println!("Memory: {}", m.memory_name);
            for issue in issues {
                println!("{}", issue);
            }
            println!();
        }
    }

    println!("------------------------------------------------");
    println!("Outdated memories:     {}", status.outdated_memories_count);
    println!("Broken wiki-links:     {}", status.broken_links_count);
    println!("Orphan memories:       {}", status.orphan_count);
    println!("Incomplete templates:  {}", status.incomplete_memories_count);
    println!("================================================");

    Ok(())
}

pub fn handle_add(
    vault_path: &Path,
    name: String,
    tags: Option<String>,
    title: Option<String>,
    global: bool,
) -> Result<(), String> {
    let memories_dir = if global {
        let global_path = crate::vault::get_global_config_path()
            .ok_or_else(|| "Could not locate global config path".to_string())?;
        let parent = global_path.parent()
            .ok_or_else(|| "Could not locate global config directory parent".to_string())?;
        let dir = parent.join("memories");
        if !dir.exists() {
            fs::create_dir_all(&dir)
                .map_err(|e| format!("Failed to create global memories directory: {}", e))?;
        }
        dir
    } else {
        let dir = vault_path.join("memories");
        if !dir.exists() {
            return Err("Vault not initialized. Run 'bw init' first.".to_string());
        }
        dir
    };

    let mut safe_name = name.trim().replace(" ", "-");
    if !safe_name.ends_with(".md") {
        safe_name.push_str(".md");
    }

    let file_path = memories_dir.join(&safe_name);
    if file_path.exists() {
        return Err(format!("Memory note at {:?} already exists.", file_path));
    }

    let parsed_tags = tags
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_else(Vec::new);

    let display_title = title.unwrap_or_else(|| {
        name.trim()
            .replace("-", " ")
            .replace("_", " ")
            .to_string()
    });

    let fm = Frontmatter {
        title: Some(display_title.clone()),
        references: Some(Vec::new()),
        tags: Some(parsed_tags),
        last_updated: Some(Utc::now().to_rfc3339()),
    };

    let page = MemoryPage {
        file_path: file_path.clone(),
        name: file_path.file_stem().unwrap().to_string_lossy().to_string(),
        frontmatter: fm,
        body: format!("# {}\n\nWrite your memory here...\n", display_title),
    };

    let serialized = serialize_memory_file(&page)?;
    fs::write(&file_path, serialized)
        .map_err(|e| format!("Failed to write memory note: {}", e))?;

    println!("SUCCESS: Created memory page at {:?}", file_path);
    Ok(())
}

pub fn handle_link(vault_path: &Path, memory: String, code_file: String) -> Result<(), String> {
    let workspace_root = get_workspace_root(vault_path);
    let code_path = workspace_root.join(&code_file);
    if !code_path.exists() {
        return Err(format!("Code file not found in workspace: {:?}", code_file));
    }

    let hash = calculate_file_hash(&code_path)?;
    let memory_file = resolve_memory_path(vault_path, &memory)?;

    let content = fs::read_to_string(&memory_file)
        .map_err(|e| format!("Failed to read memory file: {}", e))?;
    
    let mut page = parse_memory_file(&content, &memory_file)?;

    let mut refs = page.frontmatter.references.unwrap_or_default();
    
    // Check if reference already exists, if so update hash
    if let Some(pos) = refs.iter().position(|r| r.path == code_file) {
        refs[pos].hash = hash.clone();
        println!("Updating link to code file '{}' with hash '{}'", code_file, hash);
    } else {
        refs.push(CodeReference {
            path: code_file.clone(),
            hash: hash.clone(),
        });
        println!("Adding new link to code file '{}' with hash '{}'", code_file, hash);
    }

    page.frontmatter.references = Some(refs);
    page.frontmatter.last_updated = Some(Utc::now().to_rfc3339());

    let serialized = serialize_memory_file(&page)?;
    fs::write(&memory_file, serialized)
        .map_err(|e| format!("Failed to update memory file: {}", e))?;

    println!("SUCCESS: Reference linked in memory '{}'", page.name);
    Ok(())
}

pub fn handle_update(
    vault_path: &Path,
    memory: String,
    code_file: Option<String>,
) -> Result<(), String> {
    let workspace_root = get_workspace_root(vault_path);
    let memory_file = resolve_memory_path(vault_path, &memory)?;

    let content = fs::read_to_string(&memory_file)
        .map_err(|e| format!("Failed to read memory file: {}", e))?;
    
    let mut page = parse_memory_file(&content, &memory_file)?;

    let mut refs = match page.frontmatter.references {
        Some(r) => r,
        None => return Err("Memory has no references to update.".to_string()),
    };

    if let Some(target_file) = code_file {
        // Update specific reference
        let idx = refs.iter().position(|r| r.path == target_file)
            .ok_or_else(|| format!("Reference to '{}' not found in memory frontmatter.", target_file))?;

        let code_path = workspace_root.join(&target_file);
        if !code_path.exists() {
            return Err(format!("Code file not found in workspace: {}", target_file));
        }

        let new_hash = calculate_file_hash(&code_path)?;
        refs[idx].hash = new_hash;
        println!("Updated hash for '{}'", target_file);
    } else {
        // Update all references
        for r in &mut refs {
            let code_path = workspace_root.join(&r.path);
            if code_path.exists() {
                if let Ok(new_hash) = calculate_file_hash(&code_path) {
                    if r.hash != new_hash {
                        println!("Updated hash for '{}' from '{}' to '{}'", r.path, r.hash, new_hash);
                        r.hash = new_hash;
                    }
                }
            } else {
                println!("WARNING: Referenced file '{}' is missing. Skipping hash update.", r.path);
            }
        }
    }

    page.frontmatter.references = Some(refs);
    page.frontmatter.last_updated = Some(Utc::now().to_rfc3339());

    let serialized = serialize_memory_file(&page)?;
    fs::write(&memory_file, serialized)
        .map_err(|e| format!("Failed to write updated memory file: {}", e))?;

    println!("SUCCESS: Updated references for '{}'", page.name);
    Ok(())
}

pub fn handle_shake(vault_path: &Path) -> Result<(), String> {
    let memories = load_memories(vault_path)?;
    let status = check_vault_status(vault_path, &memories);

    println!("================= SHAKING VAULT =================");
    let mut broken_links_found = false;
    let mut orphans_found = false;

    for m in &status.memories {
        if !m.broken_links.is_empty() {
            broken_links_found = true;
            println!("Memory '{}' has broken wiki-links to:", m.memory_name);
            for broken in &m.broken_links {
                println!("  - [[{}]]", broken);
            }
        }

        if m.is_orphan {
            orphans_found = true;
            println!("Orphan Memory note found: '{}' ({:?})", m.memory_name, m.file_path);
        }
    }

    if !broken_links_found {
        println!("No broken wiki-links found.");
    }
    if !orphans_found {
        println!("No orphan memory notes found.");
    }

    // Clean up empty logs
    let logs_dir = vault_path.join("logs");
    let mut pruned_logs = 0;
    if logs_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(logs_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Ok(metadata) = p.metadata() {
                        if metadata.len() == 0 {
                            if fs::remove_file(&p).is_ok() {
                                pruned_logs += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    if pruned_logs > 0 {
        println!("Pruned {} empty log files from logs/", pruned_logs);
    } else {
        println!("No empty log files found to prune.");
    }

    println!("=================================================");
    Ok(())
}

pub fn handle_query(vault_path: &Path, term: String) -> Result<(), String> {
    let memories = load_memories(vault_path)?;
    let backlinks = get_backlinks(&memories);
    let term_lower = term.to_lowercase();

    println!("Query results for '{}':", term);
    println!("------------------------------------------------");

    let mut match_count = 0;
    for page in &memories {
        let title_match = page.frontmatter.title.as_ref().map(|t| t.to_lowercase().contains(&term_lower)).unwrap_or(false);
        let name_match = page.name.to_lowercase().contains(&term_lower);
        let tags_match = page.frontmatter.tags.as_ref().map(|tags| tags.iter().any(|tag| tag.to_lowercase().contains(&term_lower))).unwrap_or(false);
        let body_match = page.body.to_lowercase().contains(&term_lower);

        if name_match || title_match || tags_match || body_match {
            match_count += 1;
            println!("Memory: {} ({:?})", page.name, page.file_path.file_name().unwrap());
            if let Some(t) = &page.frontmatter.title {
                println!("  Title: {}", t);
            }
            if let Some(tags) = &page.frontmatter.tags {
                println!("  Tags: {:?}", tags);
            }
            
            // Print brief content snippets
            if body_match {
                println!("  Matching snippet:");
                for line in page.body.lines() {
                    if line.to_lowercase().contains(&term_lower) {
                        println!("    ... {} ...", line.trim());
                    }
                }
            }

            // Print backlinks
            let page_backlinks = backlinks.get(&page.name.to_lowercase());
            if let Some(bls) = page_backlinks {
                println!("  Backlinks (linked from):");
                for bl in bls {
                    println!("    - [[{}]] in {}", bl.source_name, bl.context);
                }
            }

            println!();
        }
    }

    if match_count == 0 {
        println!("No matching memory notes found.");
    } else {
        println!("Found {} matching memory notes.", match_count);
    }
    Ok(())
}

pub fn handle_read(vault_path: &Path, name: String) -> Result<(), String> {
    let memories = load_memories(vault_path)?;
    let memory_file = resolve_memory_path(vault_path, &name)?;

    let content = fs::read_to_string(&memory_file)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let page = parse_memory_file(&content, &memory_file)?;
    let workspace_root = get_workspace_root(vault_path);

    println!("=================================================");
    println!("Memory Name: {}", page.name);
    if let Some(title) = &page.frontmatter.title {
        println!("Title:       {}", title);
    }
    if let Some(tags) = &page.frontmatter.tags {
        println!("Tags:        {:?}", tags);
    }
    if let Some(ref_time) = &page.frontmatter.last_updated {
        println!("Updated:     {}", ref_time);
    }
    println!("=================================================");

    // Verify references and print status
    if let Some(refs) = &page.frontmatter.references {
        if !refs.is_empty() {
            println!("Code References:");
            for r in refs {
                let code_path = workspace_root.join(&r.path);
                let status_str = if !code_path.exists() {
                    "MISSING".to_string()
                } else {
                    match calculate_file_hash(&code_path) {
                        Ok(current_hash) => {
                            if current_hash == r.hash {
                                "OK".to_string()
                            } else {
                                format!("OUTDATED (current: {})", &current_hash[..8])
                            }
                        }
                        Err(_) => "ERROR".to_string(),
                    }
                };
                println!("  - {} -> status: {}", r.path, status_str);
            }
            println!("=================================================");
        }
    }

    // Print backlinks
    let backlinks = get_backlinks(&memories);
    if let Some(bls) = backlinks.get(&page.name.to_lowercase()) {
        println!("Backlinks (what links here):");
        for bl in bls {
            println!("  - [[{}]] (in context: {})", bl.source_name, bl.context);
        }
        println!("=================================================");
    }

    println!("\n{}", page.body);
    Ok(())
}

pub fn handle_compile(vault_path: &Path, program: String, args: Vec<String>) -> Result<(), String> {
    let programs_dir = vault_path.join("programs");
    let mut prog_file = program.clone();
    if !prog_file.ends_with(".md") {
        prog_file.push_str(".md");
    }

    let program_path = programs_dir.join(&prog_file);
    if !program_path.is_file() {
        return Err(format!("Program instruction file not found at {:?}", program_path));
    }

    let program_instructions = fs::read_to_string(&program_path)
        .map_err(|e| format!("Failed to read program file: {}", e))?;

    // Load memories to check references and include recent state
    let memories = load_memories(vault_path).unwrap_or_default();
    
    // Construct the prompt payload
    let mut compiled_prompt = String::new();
    compiled_prompt.push_str("You are an agentic application that evolves over time.\n\n");
    
    compiled_prompt.push_str("## Program Folder\n");
    compiled_prompt.push_str(&format!("Vault path is located at: {:?}\n\n", vault_path));

    if !args.is_empty() {
        compiled_prompt.push_str("## Arguments\n");
        compiled_prompt.push_str(&format!("Arguments provided: {:?}\n\n", args));
    }

    compiled_prompt.push_str("## Memories\n");
    compiled_prompt.push_str("Below are the current active memories from your database:\n");
    for page in &memories {
        compiled_prompt.push_str(&format!("=== Memory: {} ===\n", page.name));
        if let Some(title) = &page.frontmatter.title {
            compiled_prompt.push_str(&format!("Title: {}\n", title));
        }
        compiled_prompt.push_str(&format!("Content:\n{}\n\n", page.body));
    }

    compiled_prompt.push_str("## Program Instructions\n");
    compiled_prompt.push_str("Follow these step-by-step instructions to execute the task:\n");
    compiled_prompt.push_str(&program_instructions);
    
    compiled_prompt.push_str("\n\n## Reflection Loop\n");
    compiled_prompt.push_str("At the end of your run, if you changed code files, write memories or update referenced file hashes in the vault to keep memories verified.\n");

    println!("{}", compiled_prompt);
    Ok(())
}

const GLOBAL_RULES_CONTENT: &str = r#"# Antigravity Global Rules

## Brainwares Workspace Integration

Whenever a `.brainwares` vault directory is detected in the active workspace, the agent MUST automatically hook up with it:

1. **Context Discovery**:
   - Before executing tasks, check for the presence of a `.brainwares/` folder.
   - If present, run `bw status` to scan for any outdated references or broken wiki-links.
   - Automatically load and read relevant memories using `bw read <note_name>` or `bw query <term>` to gain full context about coding guidelines, rules, or system details.

2. **Reference Maintenance & Reflection**:
   - After completing edits on any codebase files, run `bw status` to see if your changes caused any memory notes to become outdated.
   - If references are outdated, inspect the associated markdown memory file, update its content to reflect the new codebase state, and run `bw update <note_name>` to synchronize the hashes.
   - If you created any new source or configuration files, proactively document them in a memory note (creating a new one if necessary) and run `bw link <note_name> <file_path>` to register their initial hashes.
   - Keep the vault clean and green before completing your turn.
"#;

const AGENT_RULES_CONTENT: &str = r#"# Brainwares Workspace Integration Rules

This repository uses **Brainwares** (`bw`) for Obsidian-style markdown memory storage and code-reference hash tracking.

You MUST follow these rules during your session:
1. **Status Audit**: Before writing or changing code, run `bw status` to check if there are any outdated memory files or broken links.
2. **Context Resolution**: Read relevant memories using `bw read <note_name>` or search memories with `bw query <term>` to gain full context about coding guidelines, system details, or database schemas.
3. **Reference Maintenance**: After modifying any code files in the codebase, run `bw status`. If any references are outdated, inspect the associated markdown note under `.brainwares/memories/`, update its contents to reflect the new codebase state, and run `bw update <note_name>` to synchronize the hashes. If you created any new source or configuration files, document them in a memory note (creating a new one if necessary) and run `bw link <note_name> <file_path>` to link and track them.
4. **Clean State**: Keep the vault clean and verified before completing your task.
"#;

pub fn handle_integrate() -> Result<(), String> {
    // 1. Configure Global Antigravity Integration
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Could not find home directory environment variable (HOME or USERPROFILE)".to_string())?;
    
    let gemini_config_dir = PathBuf::from(home).join(".gemini").join("config");
    if !gemini_config_dir.exists() {
        if let Err(e) = fs::create_dir_all(&gemini_config_dir) {
            println!("WARNING: Failed to create Gemini config directory: {}", e);
        }
    }

    let old_global_rules_str = r#"# Antigravity Global Rules

## Brainwares Workspace Integration

Whenever a `.brainwares` vault directory is detected in the active workspace, the agent MUST automatically hook up with it:

1. **Context Discovery**:
   - Before executing tasks, check for the presence of a `.brainwares/` folder.
   - If present, run `bw status` to scan for any outdated references or broken wiki-links.
   - Automatically load and read relevant memories using `bw read <note_name>` or `bw query <term>` to gain full context about coding guidelines, rules, or system details.

2. **Reference Maintenance & Reflection**:
   - After completing edits on any codebase files, run `bw status` to see if your changes caused any memory notes to become outdated.
   - If references are outdated, inspect the associated markdown memory file, update its content to reflect the new codebase state, and run `bw update <note_name>` to synchronize the hashes.
   - Keep the vault clean and green before completing your turn."#;

    if gemini_config_dir.exists() {
        let agents_md_path = gemini_config_dir.join("AGENTS.md");
        let mut current_content = String::new();
        if agents_md_path.is_file() {
            if let Ok(c) = fs::read_to_string(&agents_md_path) {
                current_content = c;
            }
        }

        if !current_content.contains("Brainwares Workspace Integration") {
            let separator = if current_content.is_empty() || current_content.ends_with('\n') { "" } else { "\n\n" };
            let new_content = format!("{}{}{}", current_content, separator, GLOBAL_RULES_CONTENT);
            if fs::write(&agents_md_path, new_content).is_ok() {
                println!("SUCCESS: Configured global Antigravity rules at {:?}", agents_md_path);
            }
        } else if current_content.contains(old_global_rules_str) {
            let updated = current_content.replace(old_global_rules_str, GLOBAL_RULES_CONTENT);
            if fs::write(&agents_md_path, updated).is_ok() {
                println!("SUCCESS: Updated global Antigravity rules at {:?}", agents_md_path);
            }
        } else if !current_content.contains("proactively document them") {
            let old_short = "If references are outdated, inspect the associated markdown memory file, update its content to reflect the new codebase state, and run `bw update <note_name>` to synchronize the hashes.\n   - Keep the vault clean and green before completing your turn.";
            let new_short = "If references are outdated, inspect the associated markdown memory file, update its content to reflect the new codebase state, and run `bw update <note_name>` to synchronize the hashes.\n   - If you created any new source or configuration files, proactively document them in a memory note (creating a new one if necessary) and run `bw link <note_name> <file_path>` to register their initial hashes.\n   - Keep the vault clean and green before completing your turn.";
            if current_content.contains(old_short) {
                let updated = current_content.replace(old_short, new_short);
                if fs::write(&agents_md_path, updated).is_ok() {
                    println!("SUCCESS: Updated global Antigravity rules at {:?}", agents_md_path);
                }
            } else {
                println!("INFO: Global Antigravity rules already configured (custom format).");
            }
        } else {
            println!("INFO: Global Antigravity rules already configured.");
        }
    }

    // 2. Configure Local Workspace Rules (CLAUDE.md, .cursorrules, .windsurfrules)
    let local_vault = PathBuf::from(".brainwares");
    if local_vault.is_dir() {
        println!("Configuring agent integration rules for local workspace...");
        
        let files_to_create = vec![
            ("CLAUDE.md", "Claude Code"),
            (".cursorrules", "Cursor"),
            (".windsurfrules", "Windsurf"),
            ("AGENTS.md", "OpenCode"),
        ];

        let old_agent_rules_str = r#"# Brainwares Workspace Integration Rules

This repository uses **Brainwares** (`bw`) for Obsidian-style markdown memory storage and code-reference hash tracking.

You MUST follow these rules during your session:
1. **Status Audit**: Before writing or changing code, run `bw status` to check if there are any outdated memory files or broken links.
2. **Context Resolution**: Read relevant memories using `bw read <note_name>` or search memories with `bw query <term>` to gain full context about coding guidelines, system details, or database schemas.
3. **Reference Maintenance**: After modifying any code files in the codebase, run `bw status`. If any references are outdated, inspect the associated markdown note under `.brainwares/memories/`, update its contents to reflect the new codebase state, and run `bw update <note_name>` to synchronize the hashes.
4. **Clean State**: Keep the vault clean and verified before completing your task."#;

        for (filename, agent_name) in files_to_create {
            let path = PathBuf::from(filename);
            let mut current = String::new();
            if path.is_file() {
                if let Ok(content) = fs::read_to_string(&path) {
                    current = content;
                }
            }

            if !current.contains("Brainwares Workspace Integration Rules") {
                let separator = if current.is_empty() || current.ends_with('\n') { "" } else { "\n\n" };
                let new_content = format!("{}{}{}", current, separator, AGENT_RULES_CONTENT);
                if fs::write(&path, new_content).is_ok() {
                    println!("SUCCESS: Configured {} integration rules in {}", agent_name, filename);
                } else {
                    println!("WARNING: Failed to write to {}", filename);
                }
            } else if current.contains(old_agent_rules_str) {
                let updated = current.replace(old_agent_rules_str, AGENT_RULES_CONTENT);
                if fs::write(&path, updated).is_ok() {
                    println!("SUCCESS: Updated {} integration rules in {}", agent_name, filename);
                } else {
                    println!("WARNING: Failed to write to {}", filename);
                }
            } else if !current.contains("document them in a memory note") {
                let old_short = "3. **Reference Maintenance**: After modifying any code files in the codebase, run `bw status`. If any references are outdated, inspect the associated markdown note under `.brainwares/memories/`, update its contents to reflect the new codebase state, and run `bw update <note_name>` to synchronize the hashes.";
                let new_short = "3. **Reference Maintenance**: After modifying any code files in the codebase, run `bw status`. If any references are outdated, inspect the associated markdown note under `.brainwares/memories/`, update its contents to reflect the new codebase state, and run `bw update <note_name>` to synchronize the hashes. If you created any new source or configuration files, document them in a memory note (creating a new one if necessary) and run `bw link <note_name> <file_path>` to link and track them.";
                if current.contains(old_short) {
                    let updated = current.replace(old_short, new_short);
                    if fs::write(&path, updated).is_ok() {
                        println!("SUCCESS: Updated {} integration rules in {}", agent_name, filename);
                    } else {
                        println!("WARNING: Failed to write to {}", filename);
                    }
                } else {
                    println!("INFO: {} rules already configured in {} (custom format).", agent_name, filename);
                }
            } else {
                println!("INFO: {} rules already configured in {}.", agent_name, filename);
            }
        }
    } else {
        println!("INFO: Local workspace .brainwares vault not found. Local rules integration skipped.");
        println!("      -> Run 'bw init' first to set up a vault, then run 'bw integrate' in the project root.");
    }

    Ok(())
}

pub fn handle_doctor() -> Result<(), String> {
    println!("Checking Brainwares system configuration...");
    println!("------------------------------------------------");

    // 1. Check PATH executable
    let mut bw_ok = std::process::Command::new("bw")
        .arg("--help")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok();

    if !bw_ok {
        bw_ok = std::process::Command::new("brainwares")
            .arg("--help")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok();
    }

    if bw_ok {
        println!("[✓] Brainwares CLI binary is executable and in your PATH.");
    } else {
        println!("[✗] Brainwares CLI binary was not found in PATH.");
        println!("    -> To fix this, run: cargo install --path .");
    }

    // 2. Check Global Agent Integration
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    
    let agents_md_path = PathBuf::from(&home).join(".gemini").join("config").join("AGENTS.md");
    let mut integration_ok = false;
    if agents_md_path.is_file() {
        if let Ok(content) = fs::read_to_string(&agents_md_path) {
            if content.contains("Brainwares Workspace Integration") {
                integration_ok = true;
            }
        }
    }

    if integration_ok {
        println!("[✓] Antigravity Global Agent rules are configured at {:?}", agents_md_path);
    } else {
        println!("[✗] Antigravity Global Agent rules are NOT configured.");
        println!("    -> To fix this, run: bw integrate");
    }

    // 3. Check Workspace initialization
    let local_vault = PathBuf::from(".brainwares");
    if local_vault.is_dir() {
        println!("[✓] Local workspace has a .brainwares vault initialized.");
        
        // 4. Check Workspace Agent Rules Files
        let workspace_rules = vec![
            ("CLAUDE.md", "Claude Code"),
            (".cursorrules", "Cursor"),
            (".windsurfrules", "Windsurf"),
        ];

        for (filename, agent_name) in workspace_rules {
            let path = PathBuf::from(filename);
            let mut configured = false;
            if path.is_file() {
                if let Ok(content) = fs::read_to_string(&path) {
                    if content.contains("Brainwares Workspace Integration Rules") {
                        configured = true;
                    }
                }
            }

            if configured {
                println!("[✓] {} integration rules are configured in local {}", agent_name, filename);
            } else {
                println!("[✗] {} integration rules are NOT configured in local {}", agent_name, filename);
                println!("    -> To fix this, run: bw integrate");
            }
        }
    } else {
        println!("[ ] Local workspace does not have a .brainwares vault initialized.");
        println!("    -> Run 'bw init' to bootstrap a vault in this project.");
    }

    // 5. Check configuration files
    if let Some(global_config_path) = crate::vault::get_global_config_path() {
        if global_config_path.is_file() {
            println!("[✓] User-wide global config found at {:?}", global_config_path);
            let global_config = crate::vault::load_global_config();
            println!("    Default vault folder name: '{}'", global_config.default_vault_dir);
            println!("    Global ignore patterns: {:?}", global_config.ignore_patterns);
        } else {
            println!("[ ] Global config not found (will be initialized upon running 'bw init').");
        }
    }

    if local_vault.is_dir() {
        if let Some(local_config) = crate::vault::load_local_config(&local_vault) {
            println!("[✓] Repository-wide local config found.");
            println!("    Local ignore patterns: {:?}", local_config.ignore_patterns);
            
            let merged = crate::vault::load_merged_config(&local_vault);
            println!("    Effective merged ignore patterns: {:?}", merged.ignore_patterns);
        } else {
            println!("[✗] Local config file config.json not found in vault.");
        }
    }

    println!("------------------------------------------------");
    Ok(())
}

pub fn handle_ui(vault_path: &Path, port: u16) -> Result<(), String> {
    let ui_dir = vault_path.join("ui");
    let src_dir = ui_dir.join("src");
    
    if !ui_dir.exists() {
        fs::create_dir_all(&ui_dir)
            .map_err(|e| format!("Failed to create UI directory: {}", e))?;
    }
    if !src_dir.exists() {
        fs::create_dir_all(&src_dir)
            .map_err(|e| format!("Failed to create UI src directory: {}", e))?;
    }

    // 1. Write package.json
    let package_json_content = r#"{
  "name": "brainwares-ui",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "react": "^19.0.0",
    "react-dom": "^19.0.0",
    "marked": "^12.0.0",
    "lucide-react": "^0.400.0"
  },
  "devDependencies": {
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "@vitejs/plugin-react": "^4.3.0",
    "vite": "^6.0.0",
    "tailwindcss": "^4.0.0-beta.8",
    "@tailwindcss/vite": "^4.0.0-beta.8"
  }
}"#;
    fs::write(ui_dir.join("package.json"), package_json_content)
        .map_err(|e| format!("Failed to write package.json: {}", e))?;

    // 1.5 Write .npmrc to isolate pnpm installations from parent workspaces
    let npmrc_content = "shared-workspace-lockfile=false\nlink-workspace-packages=false\n";
    fs::write(ui_dir.join(".npmrc"), npmrc_content)
        .map_err(|e| format!("Failed to write .npmrc: {}", e))?;

    // 1.6 Write pnpm-workspace.yaml to stop pnpm from scanning parent directories
    let pnpm_workspace_content = "packages: []\n";
    fs::write(ui_dir.join("pnpm-workspace.yaml"), pnpm_workspace_content)
        .map_err(|e| format!("Failed to write pnpm-workspace.yaml: {}", e))?;

    // 2. Write vite.config.js
    let vite_config_content = r#"import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
  ],
  server: {
    host: true
  }
})"#;
    fs::write(ui_dir.join("vite.config.js"), vite_config_content)
        .map_err(|e| format!("Failed to write vite.config.js: {}", e))?;

    // 3. Write index.html
    let index_html_content = r#"<!DOCTYPE html>
<html lang="en" class="dark">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Brainwares Vault Explorer</title>
    <style>
      body {
        margin: 0;
        background-color: #09090b;
        color: #f4f4f5;
        font-family: system-ui, -apple-system, sans-serif;
      }
    </style>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.jsx"></script>
  </body>
</html>"#;
    fs::write(ui_dir.join("index.html"), index_html_content)
        .map_err(|e| format!("Failed to write index.html: {}", e))?;

    // 4. Write src/index.css (including custom markdown styling classes)
    let index_css_content = r#"@import "tailwindcss";

.markdown-body {
  font-family: system-ui, -apple-system, sans-serif;
  color: #d4d4d8;
}
.markdown-body h1 {
  font-size: 1.875rem;
  font-weight: 700;
  margin-top: 1.5rem;
  margin-bottom: 0.75rem;
  color: #f4f4f5;
  border-bottom: 1px solid #27272a;
  padding-bottom: 0.5rem;
}
.markdown-body h2 {
  font-size: 1.5rem;
  font-weight: 600;
  margin-top: 1.25rem;
  margin-bottom: 0.75rem;
  color: #e4e4e7;
}
.markdown-body h3 {
  font-size: 1.25rem;
  font-weight: 600;
  margin-top: 1rem;
  margin-bottom: 0.5rem;
  color: #e4e4e7;
}
.markdown-body p {
  margin-bottom: 1rem;
  line-height: 1.625;
}
.markdown-body ul {
  list-style-type: disc;
  padding-left: 1.5rem;
  margin-bottom: 1rem;
}
.markdown-body ol {
  list-style-type: decimal;
  padding-left: 1.5rem;
  margin-bottom: 1rem;
}
.markdown-body li {
  margin-bottom: 0.375rem;
}
.markdown-body code {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  background-color: #18181b;
  color: #f43f5e;
  padding: 0.125rem 0.25rem;
  border-radius: 0.25rem;
  font-size: 0.875rem;
}
.markdown-body pre {
  background-color: #09090b;
  border: 1px solid #27272a;
  border-radius: 0.5rem;
  padding: 1rem;
  overflow-x: auto;
  margin-bottom: 1rem;
}
.markdown-body pre code {
  background-color: transparent;
  color: #e4e4e7;
  padding: 0;
  border-radius: 0;
  font-size: 0.875rem;
}
.markdown-body blockquote {
  border-left: 4px solid #6366f1;
  padding-left: 1rem;
  color: #a1a1aa;
  font-style: italic;
  margin-bottom: 1rem;
}
"#;
    fs::write(src_dir.join("index.css"), index_css_content)
        .map_err(|e| format!("Failed to write index.css: {}", e))?;

    // 5. Write src/main.jsx
    let main_jsx_content = r#"import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App.jsx'
import './index.css'

ReactDOM.createRoot(document.getElementById('root')).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)"#;
    fs::write(src_dir.join("main.jsx"), main_jsx_content)
        .map_err(|e| format!("Failed to write main.jsx: {}", e))?;

    // 6. Write src/App.jsx
    let app_jsx_content = r##"import React, { useState, useEffect, useRef } from 'react';
import { marked } from 'marked';
import { 
  BookOpen, Search, ShieldCheck, AlertCircle, RefreshCw, 
  Tag, Link2, Share2, Compass, Network, FileCode, CheckCircle 
} from 'lucide-react';
import data from './data.json';

const renderer = new marked.Renderer();
renderer.link = (href, title, text) => {
  if (href && href.startsWith('wiki:')) {
    const noteName = href.replace('wiki:', '');
    return `<a href="#" class="wiki-link text-indigo-400 hover:text-indigo-300 font-semibold underline decoration-indigo-500/40" data-note="${noteName}">${text}</a>`;
  }
  let out = `<a href="${href || '#'}"`;
  if (title) {
    out += ` title="${title}"`;
  }
  out += `>${text}</a>`;
  return out;
};
marked.use({ renderer });

export default function App() {
  const [memories, setMemories] = useState(data.memories || []);
  const [selectedNoteName, setSelectedNoteName] = useState('index');
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedTag, setSelectedTag] = useState(null);
  const [viewMode, setViewMode] = useState('doc');
  const [graphMode, setGraphMode] = useState(data.memories && data.memories.length > 150 ? 'local' : 'global');
  const [maxDepth, setMaxDepth] = useState(3);
  const canvasRef = useRef(null);
  const zoomInButtonRef = useRef(null);
  const zoomOutButtonRef = useRef(null);
  const zoomResetButtonRef = useRef(null);
  const selectedNoteNameRef = useRef(selectedNoteName);
  useEffect(() => {
    selectedNoteNameRef.current = selectedNoteName;
  }, [selectedNoteName]);

  const preprocessMarkdown = (text) => {
    if (!text) return '';
    return text.replace(/\[\[(.*?)\]\]/g, (match, note) => {
      const parts = note.split('|');
      const target = parts[0].trim();
      const label = parts[1] ? parts[1].trim() : target;
      const normTarget = target.toLowerCase().replace(/ /g, '-').replace(/_/g, '-');
      return `[${label}](wiki:${normTarget})`;
    });
  };

  const selectedNote = memories.find(m => m && m.name && m.name.toLowerCase() === selectedNoteName.toLowerCase()) 
    || memories.find(m => m && m.name && m.name.toLowerCase() === 'index') 
    || memories[0];

  useEffect(() => {
    if (selectedNote) {
      setSelectedNoteName(selectedNote.name);
    }
  }, [selectedNote]);

  const handleHtmlClick = (e) => {
    const target = e.target.closest('[data-note]');
    if (target) {
      e.preventDefault();
      const noteName = target.getAttribute('data-note');
      setSelectedNoteName(noteName);
    }
  };

  const allTags = Array.from(new Set(memories.flatMap(m => m?.frontmatter?.tags || [])));

  const filteredNotes = memories.filter(m => {
    if (!m) return false;
    const nameLower = (m.name || '').toLowerCase();
    const titleLower = (m.frontmatter?.title || '').toLowerCase();
    const bodyLower = (m.body || '').toLowerCase();
    const query = searchQuery.toLowerCase();
    
    const matchesSearch = searchQuery === '' || 
      nameLower.includes(query) ||
      titleLower.includes(query) ||
      bodyLower.includes(query);
    
    const matchesTag = !selectedTag || (m.frontmatter?.tags || []).includes(selectedTag);
    return matchesSearch && matchesTag;
  });

  useEffect(() => {
    if (viewMode !== 'graph' || !canvasRef.current) return;
    const canvas = canvasRef.current;
    const ctx = canvas.getContext('2d');
    
    const resizeCanvas = () => {
      canvas.width = canvas.parentElement.clientWidth;
      canvas.height = canvas.parentElement.clientHeight || 500;
    };
    resizeCanvas();
    window.addEventListener('resize', resizeCanvas);

    // Pan & Zoom values kept in animation closure for high performance (60fps with zero React state overhead)
    let zoom = graphMode === 'local' ? 1.0 : 0.6; // start slightly zoomed out in global mode
    let pan = { x: canvas.width / 2, y: canvas.height / 2 };
    let isPanning = false;
    let panStart = { x: 0, y: 0 };
    let draggedNode = null;
    let hoveredNode = null;

    // 1. Calculate nodes
    const nodes = memories.map((m) => {
      const id = (m.name || '').toLowerCase();
      const name = m.frontmatter?.title || m.name || '';
      const isGlobal = (m.file_path || '').includes('.config');
      
      // Proximity to main node (depth) determines circle sizes
      const depth = id === 'index' ? 0 : id.split('-').length;
      let nodeRadius = 8;
      if (depth === 0) {
        nodeRadius = 24;
      } else if (depth === 1) {
        nodeRadius = 16;
      } else if (depth === 2) {
        nodeRadius = 12;
      }
      
      return {
        id,
        name,
        x: 0,
        y: 0,
        radius: nodeRadius,
        isGlobal,
      };
    });

    // 2. Build Tree Structure from Node IDs and run Radial Dendrogram Algorithm
    const root = { id: 'index', children: [], parent: null };
    const nodeMap = { 'index': root };
    
    nodes.forEach(n => {
      if (n.id !== 'index') {
        nodeMap[n.id] = { id: n.id, children: [], parent: null, node: n };
      }
    });
    
    nodes.forEach(n => {
      if (n.id === 'index') return;
      const parts = n.id.split('-');
      let parentId = 'index';
      if (parts.length > 1) {
        parentId = parts.slice(0, -1).join('-');
      }
      const parentNode = nodeMap[parentId] || root;
      parentNode.children.push(nodeMap[n.id]);
      nodeMap[n.id].parent = parentNode;
    });

    const countLeaves = (node) => {
      if (node.children.length === 0) return 1;
      return node.children.reduce((sum, child) => sum + countLeaves(child), 0);
    };

    const assignRadialLayout = (node, startAngle, endAngle, depth) => {
      const radialStep = 240; // Base separation step
      
      // Proximity to main node expansion: more descendants under this node -> push it further out!
      // This increases the circumference space for dense branches to spread out.
      const subtreeLeaves = countLeaves(node);
      const leavesOffset = subtreeLeaves > 1 ? Math.sqrt(subtreeLeaves) * 45 : 0;
      const radius = depth * radialStep + leavesOffset;
      const midAngle = (startAngle + endAngle) / 2;
      
      if (node.node) {
        node.node.x = Math.cos(midAngle) * radius;
        node.node.y = Math.sin(midAngle) * radius;
      } else if (node.id === 'index') {
        const idxNode = nodes.find(n => n.id === 'index');
        if (idxNode) {
          idxNode.x = 0;
          idxNode.y = 0;
        }
      }
      
      if (node.children.length === 0) return;
      
      const angleRange = endAngle - startAngle;
      const activeRange = angleRange * 0.95; // leave buffer to prevent sibling branch overlaps
      const rangeStart = midAngle - activeRange / 2;
      
      node.children.sort((a, b) => a.id.localeCompare(b.id));
      
      const subtreeSizes = node.children.map(child => countLeaves(child));
      const totalSubtree = subtreeSizes.reduce((sum, s) => sum + s, 0) || 1;
      
      let currentAngle = rangeStart;
      node.children.forEach((child, idx) => {
        const slice = (subtreeSizes[idx] / totalSubtree) * activeRange;
        assignRadialLayout(child, currentAngle, currentAngle + slice, depth + 1);
        currentAngle += slice;
      });
    };

    // Initialize all static layouts centered on (0,0)
    assignRadialLayout(root, 0, Math.PI * 2, 0);

    // 3. Calculate edges
    const edges = [];
    memories.forEach(m => {
      if (!m) return;
      const body = m.body || '';
      const matches = body.match(/\[\[(.*?)\]\]/g) || [];
      matches.forEach(match => {
        const target = match.replace(/\[\[|\]\]/g, '').split('|')[0].trim()
          .toLowerCase().replace(/ /g, '-').replace(/_/g, '-');
        
        const source_node = nodes.find(n => n.id === m.name.toLowerCase());
        const target_node = nodes.find(n => n.id === target);
        if (source_node && target_node) {
          edges.push({
            source: source_node,
            target: target_node,
          });
        }
      });
    });

    // 4. Filter activeNodes and activeEdges based on graphMode
    let activeNodes = [];
    let activeEdges = [];
    const activeNoteName = selectedNoteNameRef.current.toLowerCase();
    
    if (graphMode === 'local' && activeNoteName) {
      const connectedIds = new Set([activeNoteName]);
      
      edges.forEach(edge => {
        if (edge.source.id === activeNoteName) {
          connectedIds.add(edge.target.id);
        }
        if (edge.target.id === activeNoteName) {
          connectedIds.add(edge.source.id);
        }
      });
      
      activeNodes = nodes.filter(n => connectedIds.has(n.id));
      activeEdges = edges.filter(e => connectedIds.has(e.source.id) && connectedIds.has(e.target.id));
      
      // In local mode, override positions to layout neighbors in a simple circle around the selected node at (0,0)
      const selectedNode = activeNodes.find(n => n.id === activeNoteName);
      if (selectedNode) {
        selectedNode.x = 0;
        selectedNode.y = 0;
      }
      const neighbors = activeNodes.filter(n => n.id !== activeNoteName);
      neighbors.forEach((node, index) => {
        const angle = (index / neighbors.length) * Math.PI * 2;
        node.x = Math.cos(angle) * 150;
        node.y = Math.sin(angle) * 150;
      });
    } else {
      activeNodes = nodes;
      activeEdges = edges;
    }

    let animationId;

    const step = () => {
      // Just clear and render! No physics velocity math = 0% CPU overhead
      ctx.clearRect(0, 0, canvas.width, canvas.height);

      ctx.save();
      ctx.translate(pan.x, pan.y);
      ctx.scale(zoom, zoom);

      // Grid background drawn dynamically in visible bounding box
      ctx.strokeStyle = '#18181b';
      ctx.lineWidth = 1 / zoom;
      const gridSize = 40;
      const startX = Math.floor((-pan.x) / (gridSize * zoom)) * gridSize - gridSize;
      const endX = startX + Math.ceil(canvas.width / (gridSize * zoom)) * gridSize + gridSize * 2;
      const startY = Math.floor((-pan.y) / (gridSize * zoom)) * gridSize - gridSize;
      const endY = startY + Math.ceil(canvas.height / (gridSize * zoom)) * gridSize + gridSize * 2;

      for (let x = startX; x < endX; x += gridSize) {
        ctx.beginPath();
        ctx.moveTo(x, startY);
        ctx.lineTo(x, endY);
        ctx.stroke();
      }
      for (let y = startY; y < endY; y += gridSize) {
        ctx.beginPath();
        ctx.moveTo(startX, y);
        ctx.lineTo(endX, y);
        ctx.stroke();
      }

      const activeId = selectedNoteNameRef.current.toLowerCase();

      // Draw edges
      activeEdges.forEach(edge => {
        const isConnectedToSelected = activeId && (
          edge.source.id === activeId || 
          edge.target.id === activeId
        );
        const isConnectedToHovered = hoveredNode && (
          edge.source.id === hoveredNode.id ||
          edge.target.id === hoveredNode.id
        );
        
        ctx.beginPath();
        ctx.moveTo(edge.source.x, edge.source.y);
        ctx.lineTo(edge.target.x, edge.target.y);
        
        if (isConnectedToSelected) {
          ctx.strokeStyle = '#818cf8'; // Glowing indigo edge for active connections
          ctx.lineWidth = 2.5 / zoom;
        } else if (isConnectedToHovered) {
          ctx.strokeStyle = '#a5b4fc'; // Soft indigo edge for hovered connections
          ctx.lineWidth = 2 / zoom;
        } else {
          ctx.strokeStyle = '#27272a'; // Faint gray edge for other connections
          ctx.lineWidth = 1 / zoom;
        }
        ctx.stroke();
      });

      // Draw nodes
      activeNodes.forEach(node => {
        const isCurrent = node.id === activeId;
        const isHovered = hoveredNode && node.id === hoveredNode.id;
        
        // Find top-level segment for branch color-coding
        const firstSegment = node.id === 'index' ? 'index' : node.id.split('-')[0];
        
        const branchColors = {
          'index': '#e2e8f0', // Zinc white for index root
          'ivy': '#38bdf8', // Light blue
          'src': '#818cf8', // Indigo
        };
        
        const palette = [
          '#34d399', // Emerald
          '#f59e0b', // Amber
          '#ec4899', // Pink
          '#06b6d4', // Cyan
          '#a855f7', // Purple
          '#f43f5e', // Rose
          '#10b981', // Green
          '#fb7185', // Soft Rose
          '#60a5fa', // Soft Blue
        ];
        
        const activeTopSegments = Array.from(new Set(
          activeNodes
            .map(n => n.id === 'index' ? 'index' : n.id.split('-')[0])
            .filter(s => s !== 'index')
        ));
        
        const segmentIndex = activeTopSegments.indexOf(firstSegment);
        const nodeColor = firstSegment === 'index' 
          ? '#e2e8f0' 
          : (branchColors[firstSegment] || palette[segmentIndex % palette.length]);

        // Draw hover glow ring
        ctx.beginPath();
        const displayRadius = node.radius + (isCurrent ? 6 : (isHovered ? 5 : 4));
        ctx.arc(node.x, node.y, displayRadius, 0, Math.PI * 2);
        ctx.fillStyle = isCurrent ? 'rgba(99, 102, 241, 0.25)' : (isHovered ? 'rgba(165, 180, 252, 0.2)' : 'rgba(39, 39, 42, 0.2)');
        ctx.fill();

        ctx.beginPath();
        ctx.arc(node.x, node.y, node.radius, 0, Math.PI * 2);
        
        if (isCurrent) {
          ctx.fillStyle = '#ffffff';
          ctx.strokeStyle = '#818cf8';
          ctx.lineWidth = 2.5 / zoom;
          ctx.stroke();
        } else if (isHovered) {
          ctx.fillStyle = '#ffffff';
          ctx.strokeStyle = nodeColor;
          ctx.lineWidth = 2 / zoom;
          ctx.stroke();
        } else {
          ctx.fillStyle = nodeColor;
        }
        ctx.fill();

        // Show label if zoomed in, local mode, current node, index node, hovered node, or ancestor of selected
        const shouldShowLabel = 
          zoom > 0.8 ||
          graphMode === 'local' || 
          isCurrent || 
          isHovered ||
          node.id === 'index' || 
          (selectedNoteNameRef.current && selectedNoteNameRef.current.toLowerCase().startsWith(node.id + '-'));

        if (shouldShowLabel) {
          ctx.font = `${(isCurrent || isHovered) ? 'bold' : ''} ${12 / zoom}px sans-serif`;
          ctx.fillStyle = (isCurrent || isHovered) ? '#ffffff' : '#a1a1aa';
          ctx.textAlign = 'center';
          ctx.fillText(node.name, node.x, node.y - node.radius - (8 / zoom));
        }
      });

      ctx.restore();

      animationId = requestAnimationFrame(step);
    };

    const getMousePos = (e) => {
      const rect = canvas.getBoundingClientRect();
      return {
        x: e.clientX - rect.left,
        y: e.clientY - rect.top,
      };
    };

    // Calculate mouse position inside virtual coordinate space
    const getVirtualMousePos = (e) => {
      const mousePos = getMousePos(e);
      return {
        x: (mousePos.x - pan.x) / zoom,
        y: (mousePos.y - pan.y) / zoom,
      };
    };

    const handleMouseDown = (e) => {
      const mousePos = getMousePos(e);
      const virtualPos = getVirtualMousePos(e);
      
      const clicked = activeNodes.find(node => {
        const dx = node.x - virtualPos.x;
        const dy = node.y - virtualPos.y;
        return Math.sqrt(dx * dx + dy * dy) < node.radius + 15;
      });

      if (clicked) {
        draggedNode = clicked;
        setSelectedNoteName(clicked.id);
      } else {
        isPanning = true;
        panStart.x = mousePos.x - pan.x;
        panStart.y = mousePos.y - pan.y;
      }
    };

    const handleMouseMove = (e) => {
      const mousePos = getMousePos(e);
      const virtualPos = getVirtualMousePos(e);
      
      // Update hovered node tracking dynamically
      hoveredNode = activeNodes.find(node => {
        const dx = node.x - virtualPos.x;
        const dy = node.y - virtualPos.y;
        return Math.sqrt(dx * dx + dy * dy) < node.radius + 12;
      }) || null;
      
      if (draggedNode) {
        draggedNode.x = virtualPos.x;
        draggedNode.y = virtualPos.y;
      } else if (isPanning) {
        pan.x = mousePos.x - panStart.x;
        pan.y = mousePos.y - panStart.y;
      }
    };

    const handleMouseUp = () => {
      draggedNode = null;
      isPanning = false;
    };

    // Mouse scroll wheel / trackpad zoom-around-cursor
    const handleWheel = (e) => {
      e.preventDefault();
      const zoomIntensity = 0.05;
      const mousePos = getMousePos(e);
      const zoomFactor = e.deltaY < 0 ? (1 + zoomIntensity) : (1 - zoomIntensity);
      const nextZoom = Math.max(0.1, Math.min(8, zoom * zoomFactor));
      
      pan.x = mousePos.x - (mousePos.x - pan.x) * (nextZoom / zoom);
      pan.y = mousePos.y - (mousePos.y - pan.y) * (nextZoom / zoom);
      zoom = nextZoom;
    };

    // Zoom Overlay Controls (Ref Bound Click Listeners)
    const handleZoomIn = () => {
      const cx = canvas.width / 2;
      const cy = canvas.height / 2;
      const nextZoom = Math.min(8, zoom * 1.25);
      pan.x = cx - (cx - pan.x) * (nextZoom / zoom);
      pan.y = cy - (cy - pan.y) * (nextZoom / zoom);
      zoom = nextZoom;
    };
    const handleZoomOut = () => {
      const cx = canvas.width / 2;
      const cy = canvas.height / 2;
      const nextZoom = Math.max(0.1, zoom / 1.25);
      pan.x = cx - (cx - pan.x) * (nextZoom / zoom);
      pan.y = cy - (cy - pan.y) * (nextZoom / zoom);
      zoom = nextZoom;
    };
    const handleZoomReset = () => {
      zoom = graphMode === 'local' ? 1.0 : 0.6;
      pan = { x: canvas.width / 2, y: canvas.height / 2 };
    };

    canvas.addEventListener('mousedown', handleMouseDown);
    canvas.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);
    canvas.addEventListener('wheel', handleWheel, { passive: false });

    const zoomInBtn = zoomInButtonRef.current;
    const zoomOutBtn = zoomOutButtonRef.current;
    const zoomResetBtn = zoomResetButtonRef.current;
    if (zoomInBtn) zoomInBtn.addEventListener('click', handleZoomIn);
    if (zoomOutBtn) zoomOutBtn.addEventListener('click', handleZoomOut);
    if (zoomResetBtn) zoomResetBtn.addEventListener('click', handleZoomReset);

    animationId = requestAnimationFrame(step);

    return () => {
      cancelAnimationFrame(animationId);
      window.removeEventListener('resize', resizeCanvas);
      canvas.removeEventListener('mousedown', handleMouseDown);
      canvas.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
      canvas.removeEventListener('wheel', handleWheel);
      if (zoomInBtn) zoomInBtn.removeEventListener('click', handleZoomIn);
      if (zoomOutBtn) zoomOutBtn.removeEventListener('click', handleZoomOut);
      if (zoomResetBtn) zoomResetBtn.removeEventListener('click', handleZoomReset);
    };
  }, [viewMode, memories, graphMode, maxDepth]);

  const totalNotes = memories.length;
  const globalNotesCount = memories.filter(m => m && (m.file_path || '').includes('.config')).length;
  const outdatedNotesCount = memories.filter(m => m && (m.frontmatter?.references || []).some(r => r.status && r.status !== 'OK')).length;

  return (
    <div className="flex h-screen bg-zinc-950 text-zinc-100 overflow-hidden font-sans">
      <div className="w-80 border-r border-zinc-900 bg-zinc-900/20 backdrop-blur-xl flex flex-col h-full select-none">
        <div className="p-5 border-b border-zinc-900 flex items-center space-x-3">
          <div className="p-2 bg-indigo-600/10 border border-indigo-500/20 rounded-xl text-indigo-400">
            <Compass size={22} className="animate-pulse" />
          </div>
          <div>
            <h1 className="text-md font-bold tracking-tight bg-gradient-to-r from-indigo-200 to-indigo-400 bg-clip-text text-transparent">
              Brainwares Vault
            </h1>
            <p className="text-xs text-zinc-500 font-mono">CLI UI v0.1.0</p>
          </div>
        </div>

        <div className="p-4 bg-zinc-900/40 border-b border-zinc-900 grid grid-cols-3 gap-2 text-center">
          <div className="p-2 bg-zinc-950/40 rounded-lg border border-zinc-900">
            <div className="text-xs text-zinc-500">Total</div>
            <div className="text-lg font-bold font-mono text-zinc-200">{totalNotes}</div>
          </div>
          <div className="p-2 bg-zinc-950/40 rounded-lg border border-zinc-900">
            <div className="text-xs text-zinc-500">Global</div>
            <div className="text-lg font-bold font-mono text-orange-500">{globalNotesCount}</div>
          </div>
          <div className="p-2 bg-zinc-950/40 rounded-lg border border-zinc-900">
            <div className="text-xs text-zinc-500">Outdated</div>
            <div className="text-lg font-bold font-mono text-red-400">{outdatedNotesCount}</div>
          </div>
        </div>

        <div className="p-4 space-y-3">
          <div className="relative">
            <Search className="absolute left-3 top-2.5 text-zinc-500" size={16} />
            <input
              type="text"
              placeholder="Search memories..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full bg-zinc-950 border border-zinc-900 rounded-lg pl-9 pr-4 py-2 text-sm text-zinc-200 placeholder-zinc-600 focus:outline-none focus:border-indigo-500/50 transition-colors"
            />
          </div>

          <div className="flex flex-wrap gap-1 items-center py-1">
            <button
              onClick={() => setSelectedTag(null)}
              className={`px-2 py-1 rounded text-xs transition-colors flex items-center space-x-1 ${!selectedTag ? 'bg-indigo-600/20 text-indigo-400 border border-indigo-500/20' : 'bg-zinc-950 text-zinc-500 hover:text-zinc-300'}`}
            >
              <span>All</span>
            </button>
            {allTags.map(tag => (
              <button
                key={tag}
                onClick={() => setSelectedTag(selectedTag === tag ? null : tag)}
                className={`px-2 py-1 rounded text-xs transition-colors flex items-center space-x-1 ${selectedTag === tag ? 'bg-indigo-600/20 text-indigo-400 border border-indigo-500/20' : 'bg-zinc-950 text-zinc-500 hover:text-zinc-300'}`}
              >
                <Tag size={10} />
                <span>{tag}</span>
              </button>
            ))}
          </div>
        </div>

        <div className="flex-1 overflow-y-auto px-4 pb-4 space-y-1">
          {filteredNotes.map(m => {
            const isCurrent = (m.name || '').toLowerCase() === (selectedNoteName || '').toLowerCase();
            const isGlobal = (m.file_path || '').includes('.config');
            const hasOutdated = (m.frontmatter?.references || []).some(r => r.status && r.status !== 'OK');

            return (
              <button
                key={m.name}
                onClick={() => setSelectedNoteName(m.name)}
                className={`w-full text-left p-3 rounded-xl transition-all duration-200 flex flex-col space-y-1 border ${isCurrent ? 'bg-indigo-600/10 border-indigo-500/40 text-indigo-200 shadow-lg shadow-indigo-500/5' : 'bg-transparent border-transparent hover:bg-zinc-900/40 hover:border-zinc-900 text-zinc-400 hover:text-zinc-200'}`}
              >
                <div className="flex justify-between items-start w-full">
                  <span className="font-medium text-sm truncate">{m.frontmatter.title || m.name}</span>
                  <div className="flex space-x-1 items-center flex-shrink-0">
                    {isGlobal && (
                      <span className="px-1.5 py-0.5 rounded text-[9px] bg-orange-950 border border-orange-500/20 text-orange-400 font-semibold font-mono">G</span>
                    )}
                    {hasOutdated && (
                      <AlertCircle size={12} className="text-red-400" />
                    )}
                  </div>
                </div>
                <div className="flex justify-between items-center w-full text-[10px] text-zinc-600 font-mono">
                  <span>[[{m.name}]]</span>
                  {m.frontmatter.tags && m.frontmatter.tags.length > 0 && (
                    <span className="truncate max-w-[120px]">#{m.frontmatter.tags[0]}</span>
                  )}
                </div>
              </button>
            );
          })}

          {filteredNotes.length === 0 && (
            <div className="p-8 text-center text-xs text-zinc-600">
              No matching memory notes.
            </div>
          )}
        </div>
      </div>

      <div className="flex-1 flex flex-col h-full bg-zinc-950 overflow-hidden relative">
        <div className="h-16 border-b border-zinc-900 px-6 flex justify-between items-center bg-zinc-900/10 backdrop-blur-md z-10 select-none">
          <div className="flex items-center space-x-4">
            <button
              onClick={() => setViewMode('doc')}
              className={`flex items-center space-x-2 px-3 py-1.5 rounded-lg text-sm transition-colors border ${viewMode === 'doc' ? 'bg-zinc-900 border-zinc-800 text-zinc-100' : 'bg-transparent border-transparent text-zinc-500 hover:text-zinc-300'}`}
            >
              <BookOpen size={16} />
              <span>Document</span>
            </button>
            <button
              onClick={() => setViewMode('graph')}
              className={`flex items-center space-x-2 px-3 py-1.5 rounded-lg text-sm transition-colors border ${viewMode === 'graph' ? 'bg-zinc-900 border-zinc-800 text-zinc-100' : 'bg-transparent border-transparent text-zinc-500 hover:text-zinc-300'}`}
            >
              <Network size={16} />
              <span>Visualizer</span>
            </button>
          </div>

          <div className="text-xs text-zinc-500 font-mono flex items-center space-x-2">
            <span>Workspace:</span>
            <span className="text-zinc-300 bg-zinc-900 px-2 py-1 rounded border border-zinc-800 truncate max-w-xs">
              {data.vault_path}
            </span>
          </div>
        </div>

        <div className="flex-1 overflow-hidden relative">
          {viewMode === 'doc' ? (
            <div className="flex h-full overflow-hidden">
              <div className="flex-1 overflow-y-auto px-10 py-8">
                {selectedNote ? (
                  <article className="max-w-3xl mx-auto prose prose-invert prose-indigo">
                    <div className="mb-8 border-b border-zinc-900 pb-6">
                      <div className="flex flex-wrap gap-2 mb-3">
                        {(selectedNote.frontmatter?.tags || []).map(t => (
                          <span key={t} className="px-2 py-0.5 rounded-full text-xs bg-zinc-900 border border-zinc-800 text-zinc-400 flex items-center space-x-1">
                            <Tag size={10} />
                            <span>{t}</span>
                          </span>
                        ))}
                        {(selectedNote.file_path || '').includes('.config') && (
                          <span className="px-2 py-0.5 rounded-full text-xs bg-orange-950 border border-orange-500/20 text-orange-400 font-semibold font-mono">
                            Global User Preference
                          </span>
                        )}
                      </div>

                      <h1 className="text-3xl font-bold tracking-tight text-zinc-100 mb-2">
                        {selectedNote.frontmatter?.title || selectedNote.name}
                      </h1>
                      
                      <div className="text-xs text-zinc-500 font-mono">
                        Last Updated: {selectedNote.frontmatter?.last_updated || 'Unknown'}
                      </div>
                    </div>

                    <div 
                      onClick={handleHtmlClick}
                      className="markdown-body text-zinc-300 leading-relaxed space-y-4"
                      dangerouslySetInnerHTML={{ __html: marked.parse(preprocessMarkdown(selectedNote.body)) }}
                    />
                  </article>
                ) : (
                  <div className="flex items-center justify-center h-full text-zinc-500">
                    No note selected. Select a note from the sidebar.
                  </div>
                )}
              </div>

              <div className="w-80 border-l border-zinc-900 bg-zinc-900/10 flex flex-col overflow-y-auto p-6 space-y-6">
                {selectedNote && (
                  <>
                    <div className="space-y-3">
                      <h3 className="text-xs font-bold uppercase tracking-wider text-zinc-500 flex items-center space-x-2">
                        <FileCode size={14} />
                        <span>Code References</span>
                      </h3>
                      
                      <div className="space-y-2">
                        {(selectedNote.frontmatter?.references || []).length > 0 ? (
                          (selectedNote.frontmatter?.references || []).map(ref => {
                            const isOk = ref.status === 'OK';
                            return (
                              <div key={ref.file_path} className="p-3 bg-zinc-900/40 border border-zinc-900 rounded-xl flex items-center justify-between">
                                <div className="min-w-0 flex-1 pr-2">
                                  <div className="text-xs font-mono truncate text-zinc-300" title={ref.file_path}>
                                    {(ref.file_path || '').split('/').pop()}
                                  </div>
                                  <div className="text-[10px] text-zinc-600 truncate">{ref.file_path}</div>
                                </div>
                                <div className="flex-shrink-0">
                                  {isOk ? (
                                    <span className="px-2 py-0.5 rounded-full text-[10px] bg-emerald-950 border border-emerald-500/20 text-emerald-400 font-medium flex items-center space-x-1">
                                      <CheckCircle size={10} />
                                      <span>OK</span>
                                    </span>
                                  ) : (
                                    <span className="px-2 py-0.5 rounded-full text-[10px] bg-red-950 border border-red-500/20 text-red-400 font-medium flex items-center space-x-1">
                                      <AlertCircle size={10} />
                                      <span>Outdated</span>
                                    </span>
                                  )}
                                </div>
                              </div>
                            );
                          })
                        ) : (
                          <div className="text-xs text-zinc-600 italic">No code references linked to this note.</div>
                        )}
                      </div>
                    </div>

                    <div className="space-y-3">
                      <h3 className="text-xs font-bold uppercase tracking-wider text-zinc-500 flex items-center space-x-2">
                        <Link2 size={14} />
                        <span>Backlinks</span>
                      </h3>

                      <div className="space-y-2">
                        {selectedNote.backlinks && selectedNote.backlinks.length > 0 ? (
                          selectedNote.backlinks.map(bl => (
                            <button
                              key={bl.source}
                              onClick={() => setSelectedNoteName(bl.source)}
                              className="w-full text-left p-3 bg-zinc-900/40 hover:bg-zinc-900/70 border border-zinc-900 hover:border-zinc-800 rounded-xl transition-all duration-200 flex flex-col space-y-1"
                            >
                              <div className="text-xs font-semibold text-zinc-300">
                                {bl.source}
                              </div>
                              <div className="text-[10px] text-zinc-500 italic truncate">
                                "{bl.context_line}"
                              </div>
                            </button>
                          ))
                        ) : (
                          <div className="text-xs text-zinc-600 italic">No incoming links to this note.</div>
                        )}
                      </div>
                    </div>
                  </>
                )}
              </div>
            </div>
          ) : (
            <div className="w-full h-full relative overflow-hidden bg-zinc-950">
              <canvas ref={canvasRef} className="block w-full h-full cursor-grab active:cursor-grabbing" />
              
              {graphMode === 'global' && (
                <div className="absolute top-6 left-6 p-1 bg-zinc-900/80 backdrop-blur-md border border-zinc-800 rounded-xl flex items-center space-x-1 select-none z-20 text-xs text-zinc-400 px-3 py-1.5">
                  <span className="font-semibold mr-2">Folder Depth:</span>
                  {[1, 2, 3, 4].map(d => (
                    <button
                      key={d}
                      onClick={() => setMaxDepth(d)}
                      className={`w-6 h-6 rounded flex items-center justify-center font-bold font-mono transition-colors ${maxDepth === d ? 'bg-indigo-600 text-zinc-100 shadow' : 'bg-transparent hover:text-zinc-200'}`}
                    >
                      {d}
                    </button>
                  ))}
                  <button
                    onClick={() => setMaxDepth(99)}
                    className={`px-2 h-6 rounded flex items-center justify-center font-bold transition-colors ${maxDepth === 99 ? 'bg-indigo-600 text-zinc-100 shadow' : 'bg-transparent hover:text-zinc-200'}`}
                  >
                    All
                  </button>
                </div>
              )}

              <div className="absolute top-6 right-6 p-1 bg-zinc-900/80 backdrop-blur-md border border-zinc-800 rounded-xl flex items-center space-x-1 select-none z-20">
                <button
                  onClick={() => setGraphMode('local')}
                  className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-all duration-200 ${graphMode === 'local' ? 'bg-indigo-600 text-zinc-100 shadow-md shadow-indigo-500/10' : 'bg-transparent text-zinc-500 hover:text-zinc-300'}`}
                >
                  Local Graph
                </button>
                <button
                  onClick={() => setGraphMode('global')}
                  className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-all duration-200 ${graphMode === 'global' ? 'bg-indigo-600 text-zinc-100 shadow-md shadow-indigo-500/10' : 'bg-transparent text-zinc-500 hover:text-zinc-300'}`}
                >
                  Global Graph
                </button>
              </div>
              
              <div className="absolute bottom-6 right-6 p-1 bg-zinc-900/80 backdrop-blur-md border border-zinc-800 rounded-xl flex items-center space-x-1 select-none z-20">
                <button
                  ref={zoomInButtonRef}
                  className="w-8 h-8 flex items-center justify-center rounded-lg text-sm font-bold text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800 transition-colors"
                  title="Zoom In"
                >
                  ＋
                </button>
                <button
                  ref={zoomOutButtonRef}
                  className="w-8 h-8 flex items-center justify-center rounded-lg text-sm font-bold text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800 transition-colors"
                  title="Zoom Out"
                >
                  －
                </button>
                <button
                  ref={zoomResetButtonRef}
                  className="w-8 h-8 flex items-center justify-center rounded-lg text-sm font-bold text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800 transition-colors"
                  title="Reset View"
                >
                  ⟲
                </button>
              </div>
              
              <div className="absolute bottom-6 left-6 p-4 bg-zinc-900/80 backdrop-blur-md border border-zinc-800 rounded-xl text-xs space-y-2 select-none text-zinc-300">
                <h4 className="font-bold text-zinc-200 mb-1">Legend</h4>
                <div className="flex items-center space-x-2">
                  <span className="w-3 h-3 rounded-full bg-indigo-400" />
                  <span>Current Node</span>
                </div>
                <div className="flex items-center space-x-2">
                  <span className="w-3 h-3 rounded-full bg-indigo-500" />
                  <span>Local Memory</span>
                </div>
                <div className="flex items-center space-x-2">
                  <span className="w-3 h-3 rounded-full bg-orange-500" />
                  <span>Global Preference</span>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}"##;
    fs::write(src_dir.join("App.jsx"), app_jsx_content)
        .map_err(|e| format!("Failed to write App.jsx: {}", e))?;

    // 7. Compile memories and generate src/data.json
    let memories = load_memories(vault_path)?;
    let workspace_root = get_workspace_root(vault_path);
    let backlinks_map = get_backlinks(&memories);
    
    let mut memories_json = Vec::new();
    for page in &memories {
        let normalized_name = crate::vault::normalize_memory_name(&page.name);
        let backlinks = backlinks_map.get(&normalized_name).cloned().unwrap_or_default();
        
        let mut refs_json = Vec::new();
        if let Some(refs) = &page.frontmatter.references {
            for code_ref in refs {
                let code_file_path = workspace_root.join(&code_ref.path);
                let status = if !code_file_path.exists() {
                    "Missing"
                } else {
                    match calculate_file_hash(&code_file_path) {
                        Ok(current_hash) => {
                            if current_hash == code_ref.hash {
                                "OK"
                            } else {
                                "Outdated"
                            }
                        }
                        Err(_) => "Missing"
                    }
                };
                
                refs_json.push(serde_json::json!({
                    "file_path": code_ref.path,
                    "status": status,
                }));
            }
        }
        
        memories_json.push(serde_json::json!({
            "name": page.name,
            "file_path": page.file_path.to_string_lossy(),
            "frontmatter": {
                "title": page.frontmatter.title,
                "tags": page.frontmatter.tags,
                "last_updated": page.frontmatter.last_updated,
                "references": refs_json,
            },
            "body": page.body,
            "backlinks": backlinks.iter().map(|bl| serde_json::json!({
                "source": bl.source_name,
                "context_line": bl.context,
            })).collect::<Vec<_>>(),
        }));
    }
    
    let data_json = serde_json::json!({
        "vault_path": vault_path.to_string_lossy(),
        "memories": memories_json,
    });
    
    let serialized_data = serde_json::to_string_pretty(&data_json)
        .map_err(|e| format!("Failed to serialize data.json: {}", e))?;
    fs::write(src_dir.join("data.json"), serialized_data)
        .map_err(|e| format!("Failed to write data.json: {}", e))?;

    // 8. Run Installation
    let node_modules_path = ui_dir.join("node_modules");
    if !node_modules_path.exists() {
        // Remove any old/broken lockfile that might have been created under monorepo scope
        let old_lockfile = ui_dir.join("pnpm-lock.yaml");
        if old_lockfile.exists() {
            let _ = fs::remove_file(old_lockfile);
        }

        println!("Installing web interface dependencies (this may take a few seconds)...");
        let install_status = std::process::Command::new("vp")
            .arg("install")
            .current_dir(&ui_dir)
            .status();
        if install_status.is_err() || !install_status.unwrap().success() {
            println!("WARNING: Failed to run 'vp install'. Trying fallback 'pnpm install'...");
            let _ = std::process::Command::new("pnpm")
                .arg("install")
                .current_dir(&ui_dir)
                .status();
        }
    }

    // 9. Start local Dev Server
    println!("Starting Brainwares visual explorer on http://localhost:{} ...", port);
    let dev_status = std::process::Command::new("vp")
        .arg("dev")
        .arg("--port")
        .arg(port.to_string())
        .current_dir(&ui_dir)
        .status();
    
    if dev_status.is_err() || !dev_status.unwrap().success() {
        println!("WARNING: Failed to run 'vp dev'. Trying fallback 'npx vite --port {}'...", port);
        let _ = std::process::Command::new("npx")
            .arg("vite")
            .arg("--port")
            .arg(port.to_string())
            .current_dir(&ui_dir)
            .status();
    }

    Ok(())
}

pub fn handle_index(vault_path: &Path) -> Result<(), String> {
    let workspace_root = get_workspace_root(vault_path);
    let merged_config = crate::vault::load_merged_config(vault_path);
    let memories_dir = vault_path.join("memories");
    
    println!("Indexing codebase under workspace: {:?}", workspace_root);
    
    // Group files by parent directory path
    let mut dir_files: std::collections::HashMap<PathBuf, Vec<crate::models::CodeReference>> = std::collections::HashMap::new();
    let mut dir_set: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    
    // Setup gitignore-aware WalkBuilder
    let mut builder = ignore::WalkBuilder::new(&workspace_root);
    builder
        .standard_filters(true)
        .hidden(true);
        
    // Add custom ignore overrides from config
    let mut override_builder = ignore::overrides::OverrideBuilder::new(&workspace_root);
    for pattern in &merged_config.ignore_patterns {
        let clean = pattern.trim_start_matches("**/").trim_end_matches('/');
        if !clean.is_empty() {
            let _ = override_builder.add(&format!("!{}", clean));
        }
    }
    if let Ok(overrides) = override_builder.build() {
        builder.overrides(overrides);
    }
    
    let walker = builder.build();
    
    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        
        // We only index files
        if !path.is_file() {
            continue;
        }
        
        // Skip common binary files
        if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy().to_lowercase();
            let skip_exts = ["png", "jpg", "jpeg", "gif", "ico", "svg", "lock", "db", "bin", "exe", "wasm", "node_modules"];
            if skip_exts.contains(&ext_str.as_str()) {
                continue;
            }
        }
        
        let parent = match path.parent() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };
        
        // Skip root directory files as the main entry note is index.md
        if parent == workspace_root {
            continue;
        }
        
        // Skip hidden paths
        if parent.components().any(|c| c.as_os_str().to_string_lossy().starts_with('.')) {
            continue;
        }
        
        let rel_path = match path.strip_prefix(&workspace_root) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => continue,
        };
        
        let hash = match crate::hash::calculate_file_hash(path) {
            Ok(h) => h,
            Err(_) => continue,
        };
        
        dir_files.entry(parent.clone()).or_default().push(crate::models::CodeReference {
            path: rel_path,
            hash,
        });
        
        // Walk up parent ancestors to build the directory tree hierarchy
        let mut current = parent;
        while current != workspace_root {
            dir_set.insert(current.clone());
            if let Some(p) = current.parent() {
                current = p.to_path_buf();
            } else {
                break;
            }
        }
    }
    
    let mut scaffolded_count = 0;
    let mut top_level_dirs = Vec::new();
    
    for dir_path in &dir_set {
        let references = dir_files.get(dir_path).cloned().unwrap_or_default();
        
        // Find direct child subdirectories in our indexed set
        let mut subdirs = Vec::new();
        for other_path in &dir_set {
            if other_path.parent() == Some(dir_path) {
                subdirs.push(other_path.clone());
            }
        }
        subdirs.sort();
        
        // Keep track of top-level directories to link in index.md
        if dir_path.parent() == Some(&workspace_root) {
            top_level_dirs.push(dir_path.clone());
        }
        
        let rel_path = match dir_path.strip_prefix(&workspace_root) {
            Ok(p) => p,
            Err(_) => continue,
        };
        
        let rel_path_str = rel_path.to_string_lossy().to_string();
        if rel_path_str.is_empty() {
            continue;
        }
        
        // Normalize name: replace path slashes with hyphens
        let normalized_note_name = rel_path_str
            .replace('\\', "-")
            .replace('/', "-");
        let memory_name = crate::vault::normalize_memory_name(&normalized_note_name);
        let note_path = memories_dir.join(format!("{}.md", memory_name));
        
        // If the note already exists, don't overwrite it
        if note_path.exists() {
            continue;
        }
        
        // Humanize title (e.g. "ivy-framework/src" -> "Ivy Framework Src")
        let title = humanize_title(&normalized_note_name);
        
        // Build frontmatter
        let frontmatter = crate::models::Frontmatter {
            title: Some(title.clone()),
            tags: Some(vec!["folder".to_string(), "index".to_string()]),
            references: if references.is_empty() { None } else { Some(references.clone()) },
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
        };
        
        // Build markdown body
        let mut body = format!("# {}\n\nScaffolded memory page for the `{}` directory.\n\n", title, rel_path_str);
        
        if !subdirs.is_empty() {
            body.push_str("## Subdirectories\n\n");
            for subdir in &subdirs {
                if let Ok(sub_rel) = subdir.strip_prefix(&workspace_root) {
                    let sub_rel_str = sub_rel.to_string_lossy().to_string();
                    let normalized_sub = sub_rel_str.replace('\\', "-").replace('/', "-");
                    let sub_memory_name = crate::vault::normalize_memory_name(&normalized_sub);
                    let sub_title = humanize_title(&normalized_sub);
                    body.push_str(&format!("- [[{}]] ({})\n", sub_memory_name, sub_title));
                }
            }
            body.push_str("\n");
        }
        
        if !references.is_empty() {
            body.push_str("## Core Files Reference Map\n\n");
            for ref_item in &references {
                let file_name = Path::new(&ref_item.path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| ref_item.path.clone());
                body.push_str(&format!("*   `{}`: [Enter description for file's role in this folder]\n", file_name));
            }
        }
        
        let memory_page = crate::models::MemoryPage {
            name: memory_name.clone(),
            frontmatter,
            body,
            file_path: note_path.clone(),
        };
        
        // Write the note to file
        let serialized = serialize_memory_file(&memory_page)?;
        fs::write(&memory_page.file_path, serialized)
            .map_err(|e| format!("Failed to write memory note: {}", e))?;
            
        println!("SUCCESS: Created memory page [[{}]] referencing {} files.", memory_name, references.len());
        scaffolded_count += 1;
    }
    
    // Sort top level dirs for deterministic indexing in index.md
    top_level_dirs.sort();
    
    // Update index.md with Codebase Directories if they are not already listed
    let index_path = memories_dir.join("index.md");
    if index_path.exists() && !top_level_dirs.is_empty() {
        if let Ok(mut content) = fs::read_to_string(&index_path) {
            if !content.contains("## Codebase Directories") {
                let mut dir_block = "\n\n## Codebase Directories\n\n".to_string();
                for dir_path in &top_level_dirs {
                    if let Ok(rel) = dir_path.strip_prefix(&workspace_root) {
                        let rel_str = rel.to_string_lossy().to_string();
                        let normalized = rel_str.replace('\\', "-").replace('/', "-");
                        let memory_name = crate::vault::normalize_memory_name(&normalized);
                        dir_block.push_str(&format!("- [[{}]]\n", memory_name));
                    }
                }
                content.push_str(&dir_block);
                let _ = fs::write(&index_path, content);
            }
        }
    }
    
    println!("------------------------------------------------");
    println!("Indexing completed. Created {} new memory notes.", scaffolded_count);
    Ok(())
}

fn humanize_title(name: &str) -> String {
    let clean_name = name.replace("[[", "[").replace("]]", "]");
    clean_name.split(&['_', '-', '/'])
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}


