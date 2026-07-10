use crate::models::{Frontmatter, MemoryPage};
use regex::Regex;
use std::path::Path;

pub fn parse_memory_file(content: &str, path: &Path) -> Result<MemoryPage, String> {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("Invalid path name: {:?}", path))?
        .to_string();

    let trimmed = content.trim_start();
    if trimmed.starts_with("---") {
        // Find second occurrence of ---
        if let Some(end_fm_idx) = trimmed[3..].find("---") {
            let fm_start_idx = 3;
            let fm_end_idx = end_fm_idx + 3;
            let yaml_content = &trimmed[fm_start_idx..fm_end_idx];
            let body = trimmed[fm_end_idx + 3..].trim_start().to_string();

            let frontmatter: Frontmatter = serde_yaml::from_str(yaml_content)
                .map_err(|e| format!("Failed to parse YAML frontmatter in {:?}: {}", path, e))?;

            return Ok(MemoryPage {
                file_path: path.to_path_buf(),
                name,
                frontmatter,
                body,
            });
        }
    }

    // No frontmatter
    Ok(MemoryPage {
        file_path: path.to_path_buf(),
        name,
        frontmatter: Frontmatter::default(),
        body: content.to_string(),
    })
}

pub fn serialize_memory_file(page: &MemoryPage) -> Result<String, String> {
    let yaml_str = serde_yaml::to_string(&page.frontmatter)
        .map_err(|e| format!("Failed to serialize YAML frontmatter: {}", e))?;
    
    // serde_yaml output starts with "---" and ends with "\n" or contains them, but let's ensure it has proper triple-dashes
    let fm_block = if yaml_str.trim().is_empty() || yaml_str.trim() == "{}" {
        "".to_string()
    } else {
        format!("---\n{}---\n\n", yaml_str.trim_start())
    };

    Ok(format!("{}{}", fm_block, page.body))
}

pub fn extract_wiki_links(body: &str) -> Vec<(String, String)> {
    let mut links = Vec::new();
    let re = Regex::new(r"\[\[([^\]#|]+)(?:#[^\]|]+)?(?:\|[^\]]+)?\]\]").unwrap();
    
    let mut in_code_block = false;
    
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        
        if in_code_block {
            continue;
        }
        
        // Strip inline code segments (e.g. `[[...slug]]` -> "")
        let clean_line = clean_inline_code(line);
        
        for cap in re.captures_iter(&clean_line) {
            if let Some(target) = cap.get(1) {
                let target_name = target.as_str().trim().to_string();
                let raw_match = cap.get(0).unwrap().as_str().to_string();
                links.push((target_name, raw_match));
            }
        }
    }
    
    links
}

fn clean_inline_code(line: &str) -> String {
    let mut clean = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    
    while i < chars.len() {
        if chars[i] == '`' {
            // Count opening backticks
            let mut count = 0;
            while i < chars.len() && chars[i] == '`' {
                count += 1;
                i += 1;
            }
            
            // Find matching closing backticks
            let mut found_match = false;
            let mut j = i;
            while j < chars.len() {
                if chars[j] == '`' {
                    let mut close_count = 0;
                    while j + close_count < chars.len() && chars[j + close_count] == '`' {
                        close_count += 1;
                    }
                    if close_count == count {
                        found_match = true;
                        i = j + close_count;
                        break;
                    } else {
                        j += close_count;
                    }
                } else {
                    j += 1;
                }
            }
            
            if !found_match {
                // If no matching closing backticks, treat opening backticks as literal text
                for _ in 0..count {
                    clean.push('`');
                }
            }
        } else {
            clean.push(chars[i]);
            i += 1;
        }
    }
    
    clean
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_with_frontmatter() {
        let content = "\
---
title: Auth Page
tags: [auth, test]
references:
  - path: src/auth.rs
    hash: 1234abcd
---
# Main Header
Body text with [[Another Note#Section|Label]].
";
        let path = PathBuf::from("memories/auth-page.md");
        let page = parse_memory_file(content, &path).unwrap();
        assert_eq!(page.name, "auth-page");
        assert_eq!(page.frontmatter.title.as_deref(), Some("Auth Page"));
        assert_eq!(page.frontmatter.tags.unwrap(), vec!["auth", "test"]);
        assert_eq!(page.frontmatter.references.unwrap()[0].path, "src/auth.rs");
        assert!(page.body.contains("# Main Header"));

        let links = extract_wiki_links(&page.body);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].0, "Another Note");
        assert_eq!(links[0].1, "[[Another Note#Section|Label]]");
    }

    #[test]
    fn test_extract_wiki_links_ignores_code() {
        let body = "\
This is a normal [[Active Link]].
Inline code `[[Ignored Inline Link]]` or `[[...slug]]`.
Multiple backticks: `` `[[Another Ignored]]` ``.
Code block:
```rust
let x = [[Ignored Code Block Link]];
```
Normal link after code block: [[Active Link 2]].
";
        let links = extract_wiki_links(body);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].0, "Active Link");
        assert_eq!(links[1].0, "Active Link 2");
    }
}
