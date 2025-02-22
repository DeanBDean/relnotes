#![deny(clippy::unimplemented, clippy::ok_expect, clippy::mem_forget)]
// Our standard Clippy lints that we use in Embark projects, we opt out of a few that are not appropriate for the specific crate (yet)
#![warn(
    clippy::all,
    clippy::doc_markdown,
    clippy::dbg_macro,
    clippy::todo,
    clippy::empty_enum,
    clippy::enum_glob_use,
    clippy::pub_enum_variant_names,
    clippy::mem_forget,
    clippy::use_self,
    clippy::filter_map_next,
    clippy::needless_continue,
    clippy::needless_borrow,
    clippy::match_wildcard_for_single_variants,
    clippy::if_let_mutex,
    clippy::mismatched_target_os,
    clippy::await_holding_lock,
    clippy::match_on_vec_items,
    clippy::imprecise_flops,
    clippy::suboptimal_flops,
    clippy::lossy_float_literal,
    clippy::rest_pat_in_fully_bound_structs,
    clippy::fn_params_excessive_bools,
    clippy::exit,
    clippy::inefficient_to_string,
    clippy::linkedlist,
    clippy::macro_use_imports,
    clippy::option_option,
    clippy::verbose_file_reads,
    clippy::unnested_or_patterns,
    rust_2018_idioms,
    future_incompatible,
    nonstandard_style
)]

mod config;
mod data;

use std::path::PathBuf;

use octocrab::Octocrab;
use structopt::StructOpt;

use config::timeframe::Timeframe;

#[derive(StructOpt)]
/// Generate release notes for your project.
struct Cli {
    /// Path to the configuration file. (Default: `None`)
    #[structopt(short, long, parse(from_os_str))]
    config: Option<PathBuf>,
    /// The GitHub authenication token. (Default: `None`)
    #[structopt(short, long)]
    token: Option<String>,
    /// The start of the new release timeframe. Default: `release:latest`.
    #[structopt(long)]
    from: Option<Timeframe>,
    /// The end of the new release timeframe. Default: `today`.
    #[structopt(long)]
    to: Option<Timeframe>,
    /// Skip PRs if their labels match the regular expressions.
    #[structopt(long)]
    skip_labels: Option<Vec<String>>,
    /// The repository and new version to generate release notes in the
    /// form `owner/repo@version`. `owner/repo@` is optional if provided
    /// a configuration file.
    repo_and_version: String,
}

fn initialise_github(token: Option<String>) -> eyre::Result<Octocrab> {
    let mut builder = octocrab::Octocrab::builder();
    let token = token.or_else(|| std::env::var("GITHUB_TOKEN").ok());
    if token.is_some() {
        builder = builder.personal_token(token.unwrap());
    }
    Ok(builder.build()?)
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let cli = Cli::from_args();
    let path = cli.config.map(|path| path.canonicalize()).transpose()?;

    let (mut config, version) = if let Some(path) = path {
        log::info!("Using configuration file found at `{}`.", path.display());
        let string = tokio::fs::read_to_string(path).await?;
        (
            toml::from_str::<config::Config>(&string)?,
            cli.repo_and_version,
        )
    } else {
        let regex = regex::Regex::new(r"(?P<owner>\S+)/(?P<repo>\S+)@(?P<version>\S+)").unwrap();
        let cap = regex.captures(&cli.repo_and_version).ok_or_else(|| {
            eyre::eyre!("<repo_and_version> must be in `owner/repo@version` format.")
        })?;
        let owner = cap.name("owner").unwrap().as_str().to_owned();
        let repo = cap.name("repo").unwrap().as_str().to_owned();
        let version = cap.name("version").unwrap().as_str().to_owned();

        (config::Config::new(owner, repo), version)
    };

    config.from = cli.from.unwrap_or(config.from);
    config.to = cli.to.unwrap_or(config.to);
    config.skip_labels = cli
        .skip_labels
        .map(regex::RegexSet::new)
        .transpose()?
        .unwrap_or(config.skip_labels);

    log::info!("Using `{}` as version number.", version);
    let octocrab = initialise_github(cli.token)?;
    let data = data::Data::from_config(&octocrab, version, &config).await?;
    println!(
        "{}",
        tera::Tera::one_off(
            &config.template,
            &tera::Context::from_serialize(data)?,
            false
        )?
    );

    Ok(())
}
