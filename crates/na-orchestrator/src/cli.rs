use clap::{Parser, Subcommand};
use clap_complete::Shell;

#[derive(Parser)]
#[command(name = "ngalir", version, about = "Flow automation engine")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Execute a Flow Spec
    Run {
        /// Path to Flow Spec YAML file
        flow: String,
        /// Directory for checkpoint state files (enables resume on restart)
        #[arg(long)]
        state_dir: Option<String>,
        /// JSON input string to inject as `__request__` for the flow
        #[arg(long)]
        input: Option<String>,
        /// Port for Prometheus metrics HTTP server (disabled if 0)
        #[arg(long, default_value_t = 0)]
        metrics_port: u16,
    },
    /// List all available na-* node binaries on PATH / NGALIR_NODE_PATH
    Nodes,
    /// Validate a Flow Spec without executing it
    Validate {
        /// Path to Flow Spec YAML file
        flow: String,
    },
    /// Output the full node skills registry as JSON (for AI context)
    Skills,
    /// Generate a flow from a natural-language prompt
    Generate {
        /// Natural-language description of what the flow should do
        prompt: String,
        /// Edit an existing flow file (pass --edit path/to/flow.yaml)
        #[arg(long)]
        edit: Option<String>,
        /// LLM model to use (default: gpt-4o)
        #[arg(long)]
        model: Option<String>,
        /// Output file path (default: stdout)
        #[arg(long)]
        output: Option<String>,
    },
    /// Start the web UI server
    Serve {
        /// Port to listen on
        #[arg(long, default_value_t = 8080)]
        port: u16,
        /// Directory containing built UI files
        #[arg(long, default_value = "./ui/dist")]
        ui_dir: String,
    },
    /// Analyze a flow and suggest optimizations
    Optimize {
        /// Path to Flow Spec YAML file
        flow: String,
        /// LLM model to use (default: gpt-4o)
        #[arg(long)]
        model: Option<String>,
        /// Output file path (default: stdout)
        #[arg(long)]
        output: Option<String>,
    },
    /// Generate a new node crate scaffold from interactive prompts
    InitNode,
    /// Generate shell completions
    Completion {
        /// Shell type (bash, zsh, fish, powershell, elvish)
        shell: Shell,
    },
    /// Search the node registry for available nodes
    Search {
        /// Keyword to search for (matches name, description, use_cases)
        keyword: String,
    },
    /// Install a node binary from the registry
    Install {
        /// Node name to install (e.g. "slack" installs na-slack)
        name: String,
    },
}
