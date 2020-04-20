use futures::future::*;
use chrono::{DateTime, Utc};
use chrono_humanize::HumanTime;
use clap::{App, Arg, SubCommand};
use colored::*;
use itertools::Itertools;

use gitlab::*;
use std::time::Instant;

const EMPTY_PARAMS: &[(&str, &str)] = &[];

mod config;
use config::Config;

#[derive(Clone)]
pub struct EnvironmentRow {
    pub project_name: String,
    pub environment_name: String,
    pub deployment_by: String,
    pub commit_sha: String,
    pub updated: String,
}

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
    let name: &String = &project_name_and_id.0;
    let id: &ProjectId = &project_name_and_id.1;
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

async fn build_environment_row(
    gitlab: &Gitlab,
    project_name: &str,
    project_id: ProjectId,
    env: &Environment,
) -> Result<EnvironmentRow, String> {
    let env: Environment = gitlab
        .environment(project_id, env.id, EMPTY_PARAMS)
        .unwrap();
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
    Ok(EnvironmentRow {
        project_name: project_name.to_owned(),
        environment_name: env.name,
        deployment_by: iid,
        commit_sha: commit,
        updated,
    })
}

fn all_the_same(results: &[EnvironmentRow]) -> bool {
    let mut commits: Vec<String> = results.iter().map(|x| x.commit_sha.clone()).collect();
    commits.dedup();
    commits.len() == 1
}

async fn get_environment_details(
    gitlab: &Gitlab,
    all_envs: Vec<Vec<(String, ProjectId, Environment)>>,
) -> Result<Vec<EnvironmentRow>,String> {
    let before = Instant::now();

    join_all(all_envs.iter().flat_map::<Vec<_>, _>(|envs_of_project| {
        envs_of_project
            .iter()
            .map(|x| build_environment_row(gitlab, &x.0, x.1, &x.2))
            .collect()
    }))
    .inspect(|_| println!("Retrieved environments details [{:.2?}]", before.elapsed()))
    .await
    .into_iter()
    .collect()
}

#[tokio::main]
async fn main() -> Result<(),String> {
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
        println!("about to start");
        let gitlab_fut = async {
            Gitlab::new(config.server, config.access_token).map_err(|_err| "Could not connect")
        };
        println!("future defined");
        let gitlab = gitlab_fut.await?;
        println!("future awaited");
        let results = get_projects_for_namespace(&gitlab, namespace)
            .then(|project_names| get_all_environments(&gitlab, project_names))
            .then(|all_envs| get_environment_details(&gitlab, all_envs))
            .await?;
        let results: Vec<&EnvironmentRow> = results
            .iter()
            .filter(|x| !x.commit_sha.is_empty())
            .collect();
        // Early return if there is nothing to show
        if results.is_empty() {
            println!("There is nothing to show");
            return Ok(());
        }

        // Show results otherwise
        let longest_project = results
            .iter()
            .map(|x| x.project_name.len())
            .max()
            .unwrap()
            .max(7);
        let longest_env = results
            .iter()
            .map(|x| x.environment_name.len())
            .max()
            .unwrap()
            .max(11);
        let longest_depl = results
            .iter()
            .map(|x| x.deployment_by.len())
            .max()
            .unwrap()
            .max(10);
        let longest_commit = results
            .iter()
            .map(|x| x.commit_sha.len())
            .max()
            .unwrap()
            .max(6);
        let longest_updated = results
            .iter()
            .map(|x| x.updated.len())
            .max()
            .unwrap()
            .max(7);
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
        let groups = results
            .into_iter()
            .group_by(|r| r.project_name.clone())
            .into_iter()
            .map(|(_, group)| group.cloned().collect())
            .collect::<Vec<Vec<EnvironmentRow>>>();
        for group in groups {
            let color = if all_the_same(&group) { "green" } else { "red" };
            group.into_iter().for_each(|r| {
                    println!(
                        "{:longest_project$}  {:longest_env$}  {:longest_depl$}  {:longest_commit$}  {:longest_updated$}",
                        r.project_name.color(color),
                        r.environment_name.color(color),
                        r.deployment_by.color(color),
                        r.commit_sha.color(color),
                        r.updated.color(color),
                        longest_project = longest_project,
                        longest_env = longest_env,
                        longest_depl = longest_depl,
                        longest_commit = longest_commit,
                        longest_updated = longest_updated
                    )
                })
        }
    } else {
        println!("Why don't you try the get command?")
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single_elem_vec() -> Vec<EnvironmentRow> {
        vec![EnvironmentRow {
            project_name: "project".to_string(),
            environment_name: "env".to_string(),
            deployment_by: "deployed by someone".to_string(),
            commit_sha: "asdflkj".to_string(),
            updated: "some time ago".to_string(),
        }]
    }

    #[test]
    fn test_single_elem() {
        assert!(all_the_same(&single_elem_vec()));
    }

    #[test]
    fn test_duplicates() {
        assert!(all_the_same(
            &[single_elem_vec(), single_elem_vec()].concat()
        ));
    }

    #[test]
    fn test_differences() {
        assert!(!all_the_same(
            &[
                vec![EnvironmentRow {
                    commit_sha: "fooo".to_string(),
                    ..single_elem_vec().first().unwrap().to_owned()
                }],
                single_elem_vec()
            ]
            .concat()
        ));
    }
}
