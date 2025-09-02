//! LangGraph CLI - Command-line interface for LangGraph

use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod commands;

use commands::*;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(name = "langgraph")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new LangGraph project
    Init {
        /// Project name
        name: String,
        /// Project directory (defaults to current directory)
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Template to use
        #[arg(short, long, default_value = "basic")]
        template: String,
    },
    /// Validate a graph definition
    Validate {
        /// Path to graph file
        file: PathBuf,
    },
    /// Run a graph
    Run {
        /// Path to graph file
        file: PathBuf,
        /// Input JSON for the graph
        #[arg(short, long)]
        input: Option<String>,
        /// Stream output
        #[arg(short, long)]
        stream: bool,
        /// Thread ID for checkpointing
        #[arg(short, long)]
        thread_id: Option<String>,
    },
    /// Visualize a graph
    Visualize {
        /// Path to graph file
        file: PathBuf,
        /// Output format (dot, png, svg)
        #[arg(short, long, default_value = "dot")]
        format: String,
        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Manage checkpoints
    Checkpoint {
        #[command(subcommand)]
        action: CheckpointAction,
    },
}

#[derive(Subcommand)]
enum CheckpointAction {
    /// List checkpoints
    List {
        /// Thread ID to filter by
        #[arg(short, long)]
        thread_id: Option<String>,
    },
    /// Show checkpoint details
    Show {
        /// Thread ID
        thread_id: String,
        /// Checkpoint ID
        checkpoint_id: String,
    },
    /// Delete checkpoint
    Delete {
        /// Thread ID
        thread_id: String,
        /// Checkpoint ID (optional - deletes all if not specified)
        checkpoint_id: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Init { name, path, template } => {
            init_project(name, path, template).await
        }
        Commands::Validate { file } => {
            validate_graph(file).await
        }
        Commands::Run { file, input, stream, thread_id } => {
            run_graph(file, input, stream, thread_id).await
        }
        Commands::Visualize { file, format, output } => {
            visualize_graph(file, format, output).await
        }
        Commands::Checkpoint { action } => {
            match action {
                CheckpointAction::List { thread_id } => {
                    list_checkpoints(thread_id).await
                }
                CheckpointAction::Show { thread_id, checkpoint_id } => {
                    show_checkpoint(thread_id, checkpoint_id).await
                }
                CheckpointAction::Delete { thread_id, checkpoint_id } => {
                    delete_checkpoint(thread_id, checkpoint_id).await
                }
            }
        }
    }
}
