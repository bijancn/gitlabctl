use chrono::{DateTime, Utc};
use chrono_humanize::HumanTime;
use clap::{App, Arg, SubCommand};
use colored::*;
use futures_util::future::*;

use gitlab::*;
use std::time::Instant;

const EMPTY_PARAMS: &[(&str, &str)] = &[];

mod config;
use config::Config;

async fn get_projects_for_namespace(gitlab: &Gitlab, namespace: &str) -> Vec<(String, ProjectId)> {
    let before = Instant::now();
    // There is no way to filter projects by namespace in the query parameters in v4
    let result = gitlab
        .projects(EMPTY_PARAMS)
        .unwrap_or_default()
        .iter()
        .filter(|p| {
            namespace.is_empty() || p.namespace.name.to_uppercase() == namespace.to_uppercase()
        })
        .map(|x| (x.name.to_owned(), x.id))
        .collect::<Vec<(String, ProjectId)>>();
    println!(
        "Retrieved {:} projects          [{:.2?}]",
        result.len(),
        before.elapsed()
    );
    result
}

async fn get_environments_of_project(
    gitlab: &Gitlab,
    project_name_and_id: &(String, ProjectId),
) -> Vec<(String, ProjectId, Environment)> {
    let ref name: String = project_name_and_id.0;
    let ref id: ProjectId = project_name_and_id.1;
    gitlab
        .environments(*id, EMPTY_PARAMS)
        .unwrap_or_default()
        .iter()
        .map(move |e: &Environment| (name.to_owned(), id.to_owned(), e.to_owned()))
        .collect()
}

async fn get_all_environments(
    gitlab: &Gitlab,
    project_names: Vec<(String, ProjectId)>,
) -> Vec<Vec<(String, ProjectId, Environment)>> {
    let before = Instant::now();
    let results = project_names
        .iter()
        .map(|name| get_environments_of_project(&gitlab, name));

    join_all(results)
        .inspect(|e| {
            println!(
                "Retrieved {:} environments      [{:.2?}]",
                e.iter().map(|x| x.len()).sum::<usize>(),
                before.elapsed()
            )
        })
        .await
}


async fn get_environment_details(
    gitlab: &Gitlab,
    all_envs: Vec<Vec<(String, ProjectId, Environment)>>,
) -> Result<Vec<(String, (String, String, String, String, String))>> {
    let before = Instant::now();
    let result: Vec<(String, (String, String, String, String, String))> = all_envs
        .iter()
        .flat_map::<Vec<_>, _>(|envs_of_project| {
            let result: Vec<(String, String, String, String, String)> = envs_of_project
                .iter()
                .map(
                    |(project_name, project_id, env): &(String, ProjectId, Environment)| {
                        let env: Environment =
                            gitlab.environment(project_id.to_owned(), env.id, EMPTY_PARAMS).unwrap();
                        let last_deployment: Option<Deployment> = env.last_deployment;
                        let iid: String = last_deployment
                            .to_owned()
                            .map(|deployment| {
                                let username = deployment.user.username.to_string();
                                deployment.iid.to_string() + " by " + &(username)
                            })
                            .unwrap_or_default();
                        let commit: String = last_deployment
                            .to_owned()
                            .and_then(|x: Deployment| x.deployable.commit.short_id)
                            .unwrap_or_default();
                        let now = Utc::now();
                        let updated: String = last_deployment
                            .to_owned()
                            .map(|x| DateTime::parse_from_rfc3339(&x.created_at).unwrap())
                            .map(|x| HumanTime::from(x.signed_duration_since(now)).to_string())
                            .unwrap_or_default();
                        (
                            project_name.to_owned(),
                            env.name,
                            iid,
                            commit,
                            updated,
                            last_deployment.to_owned(),
                        )
                    },
                )
                .filter(|x| !x.3.is_empty())
                .map(|x| (x.0, x.1, x.2, x.3, x.4))
                .collect();
            let mut commits: Vec<String> = (result).iter().map(|x| x.3.to_owned()).collect();
            commits.dedup();
            if commits.len() == 1 {
                result
                    .iter()
                    .map(|r| (String::from("green"), r.to_owned()))
                    .collect()
            } else {
                result
                    .iter()
                    .map(|r| (String::from("red"), r.to_owned()))
                    .collect()
            }
        })
        .collect();
    println!("Retrieved environments details [{:.2?}]", before.elapsed());
    Ok(result)
}

#[tokio::main]
async fn main() -> Result<()> {
    let matches = App::new("gitlabctl")
        .version("0.1")
        .author("Bijan Chokoufe Nejad <bijan@chokoufe.com>")
        .about("gitlabctl controls gitlab from the command line")
        .subcommand(
            SubCommand::with_name("get")
                .about("get resources from gitlab")
                .arg(
                    Arg::with_name("resource")
                        .help("The resource to get, e.g. environment.")
                        .required(true)
                        .index(1),
                )
                .arg(
                    Arg::with_name("namespace")
                        .short("n")
                        .long("namespace")
                        .help("Filters the resources to the given namespace/group.")
                        .takes_value(true),
                ),
        )
        .get_matches();
    if let Some(matches) = matches.subcommand_matches("get") {
        let namespace = matches.value_of("namespace").unwrap_or_default();
        let config = Config::parse_from_disk();
        let gitlab = Gitlab::new(config.server, config.access_token)?;
        let results = get_projects_for_namespace(&gitlab, namespace)
            .then(|project_names| get_all_environments(&gitlab, project_names))
            .then(|all_envs| get_environment_details(&gitlab, all_envs))
            .await?;
        // Early return if there is nothing to show
        if results.is_empty() {
            return Ok(println!("There is nothing to show"));
        }

        // Show results otherwise
        let longest_project = results.iter().map(|x| (x.1).0.len()).max().unwrap().max(7);
        let longest_env = results.iter().map(|x| (x.1).1.len()).max().unwrap().max(11);
        let longest_depl = results.iter().map(|x| (x.1).2.len()).max().unwrap().max(10);
        let longest_commit = results.iter().map(|x| (x.1).3.len()).max().unwrap().max(6);
        let longest_updated = results.iter().map(|x| (x.1).4.len()).max().unwrap().max(7);
        println!(
                    "{:longest_project$}  {:longest_env$}  {:longest_depl$}  {:longest_commit$}  {:longest_updated$}",
                    "PROJECT",
                    "ENVIRONMENT",
                    "DEPLOYMENT",
                    "COMMIT",
                    "UPDATED",
                    longest_project = longest_project,
                    longest_env = longest_env,
                    longest_depl = longest_depl,
                    longest_commit = longest_commit,
                    longest_updated = longest_updated
                );
        Ok(results.iter().for_each(|r| {
                    println!(
                        "{:longest_project$}  {:longest_env$}  {:longest_depl$}  {:longest_commit$}  {:longest_updated$}",
                        (r.1).0.color(r.0.to_owned()),
                        (r.1).1.color(r.0.to_owned()),
                        (r.1).2.color(r.0.to_owned()),
                        (r.1).3.color(r.0.to_owned()),
                        (r.1).4.color(r.0.to_owned()),
                        longest_project = longest_project,
                        longest_env = longest_env,
                        longest_depl = longest_depl,
                        longest_commit = longest_commit,
                        longest_updated = longest_updated
                    )
                }))
    } else {
        Ok(println!("Why don't you try the get command?"))
    }
}
