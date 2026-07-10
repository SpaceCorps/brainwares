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
    let memories_dir = vault_path.join("memories");
    
    // Check if input is already a path pointing to a file
    let path = PathBuf::from(input);
    if path.is_file() {
        return Ok(path);
    }

    // Otherwise look up in memories dir
    let mut file_name = input.to_string();
    if !file_name.ends_with(".md") {
        file_name.push_str(".md");
    }

    let resolved = memories_dir.join(&file_name);
    if resolved.is_file() {
        return Ok(resolved);
    }

    // Try lowercased lookup as fallback
    if let Ok(entries) = fs::read_dir(&memories_dir) {
        let input_lower = file_name.to_lowercase();
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() && p.file_name().and_then(|n| n.to_str()).map(|s| s.to_lowercase()) == Some(input_lower.clone()) {
                return Ok(p);
            }
        }
    }

    Err(format!(
        "Memory file '{}' not found in memories directory {:?}",
        input, memories_dir
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

        if !issues.is_empty() {
            println!("Memory: {}", m.memory_name);
            for issue in issues {
                println!("{}", issue);
            }
            println!();
        }
    }

    println!("------------------------------------------------");
    println!("Outdated memories:  {}", status.outdated_memories_count);
    println!("Broken wiki-links:  {}", status.broken_links_count);
    println!("Orphan memories:    {}", status.orphan_count);
    println!("================================================");

    Ok(())
}

pub fn handle_add(
    vault_path: &Path,
    name: String,
    tags: Option<String>,
    title: Option<String>,
) -> Result<(), String> {
    let memories_dir = vault_path.join("memories");
    if !memories_dir.exists() {
        return Err("Vault not initialized. Run 'bw init' first.".to_string());
    }

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
   - Keep the vault clean and green before completing your turn.
"#;

pub fn handle_integrate() -> Result<(), String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Could not find home directory environment variable (HOME or USERPROFILE)".to_string())?;
    
    let gemini_config_dir = PathBuf::from(home).join(".gemini").join("config");
    if !gemini_config_dir.exists() {
        fs::create_dir_all(&gemini_config_dir)
            .map_err(|e| format!("Failed to create Gemini config directory at {:?}: {}", gemini_config_dir, e))?;
    }

    let agents_md_path = gemini_config_dir.join("AGENTS.md");
    let mut current_content = String::new();
    if agents_md_path.is_file() {
        current_content = fs::read_to_string(&agents_md_path)
            .map_err(|e| format!("Failed to read existing AGENTS.md: {}", e))?;
    }

    if current_content.contains("Brainwares Workspace Integration") {
        println!("INFO: Brainwares integration already configured in global AGENTS.md.");
        return Ok(());
    }

    let separator = if current_content.is_empty() || current_content.ends_with('\n') { "" } else { "\n\n" };
    let new_content = format!("{}{}{}", current_content, separator, GLOBAL_RULES_CONTENT);
    
    fs::write(&agents_md_path, new_content)
        .map_err(|e| format!("Failed to write global AGENTS.md file: {}", e))?;

    println!("SUCCESS: Global Antigravity rules configured successfully at {:?}", agents_md_path);
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
        // Try fallback to brainwares
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

    // 2. Check Agent Integration
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
    } else {
        println!("[ ] Local workspace does not have a .brainwares vault initialized (optional).");
        println!("    -> Run 'bw init' to bootstrap a vault in this project.");
    }

    println!("------------------------------------------------");
    Ok(())
}

