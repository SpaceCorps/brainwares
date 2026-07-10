use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "bw")]
#[command(about = "Brainwares: Markdown-based memory storage & code-reference hashing CLI for AI agents", long_about = None)]
pub struct Cli {
    #[arg(short, long, global = true, help = "Path to the vault directory (defaults to searching for .brainwares)")]
    pub vault: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(about = "Initialize a .brainwares vault in the current directory")]
    Init,

    #[command(about = "Scan references, validate hashes, check wiki-links, and print vault status")]
    Status,

    #[command(about = "Create a new memory note")]
    Add {
        #[arg(help = "Name of the memory note (e.g. 'auth-flow')")]
        name: String,
        
        #[arg(short, long, help = "Comma-separated list of tags")]
        tags: Option<String>,
        
        #[arg(short, long, help = "Title of the memory note")]
        title: Option<String>,
    },

    #[command(about = "Link a code file reference to a memory note")]
    Link {
        #[arg(help = "Name or file path of the memory note")]
        memory: String,
        
        #[arg(help = "Relative path to the code file to reference")]
        code_file: String,
    },

    #[command(about = "Update code references in a memory note to their current hashes")]
    Update {
        #[arg(help = "Name or file path of the memory note")]
        memory: String,
        
        #[arg(help = "Optional: Path to the specific code file to update. If omitted, updates all references.")]
        code_file: Option<String>,
    },

    #[command(about = "Clean up dead wiki-links, report orphan pages, and clean up temporary logs")]
    Shake,

    #[command(about = "Search/query memories by keyword, tags, or references")]
    Query {
        #[arg(help = "Search query term")]
        term: String,
    },

    #[command(about = "Read a memory note, including details, current hash status, and backlinks")]
    Read {
        #[arg(help = "Name or file path of the memory note")]
        name: String,
    },

    #[command(about = "Compile a Promptware program with Firmware + Program + Memory context")]
    Compile {
        #[arg(help = "Name of the program file under programs/ (e.g. 'refactor')")]
        program: String,
        
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, help = "Additional arguments to forward to the program template")]
        args: Vec<String>,
    },

    #[command(about = "Integrate Brainwares with global Antigravity coding agent configuration")]
    Integrate,

    #[command(about = "Verify that Brainwares CLI and agent integrations are set up correctly")]
    Doctor,
}
