//! Scan the Job-PR JSON for the Run IDs that were updated UTC 00:00 or later: https://github.com/lupyuen/nuttx-github-jobs/blob/main/nuttx-github-jobs.json
//! For Each Run ID: Scan the success / warning / error folders to fetch all Target Groups, like arm-01: https://github.com/lupyuen/nuttx-github-jobs
//! For Each Run ID and Target Group: Scan the success / warning / error folders for the min and max timestamp.
//! For Each Run ID and Target Group: Compute the GitHub Runner Minutes based on max timestamp - min timestamp.
//! For Each Run ID: Add up the GitHub Runner Minutes for all Target Groups.
//! Add up all Target Groups to get the Total GitHub Runner Minutes since UTC 00:00.
use std::{collections::{HashMap, HashSet}, fs::read_dir};
use struson::{
    json_path,
    reader::{JsonReader, JsonStreamReader, simple::{SimpleJsonReader, ValueReader}},
    writer::{JsonStreamWriter, JsonWriter}
};

/// JSON File that contains the Job-PR records for all NuttX GitHub Jobs
const JOB_PR_JSON: &str = "../nuttx-github-jobs/nuttx-github-jobs.json";

fn main() {
    // Scan the Job-PR JSON for the Run IDs that were updated UTC 00:00 or later: https://github.com/lupyuen/nuttx-github-jobs/blob/main/nuttx-github-jobs.json
    let (recent_jobs, _) = fetch_recent_jobs();
    println!("Recent Jobs: {recent_jobs:?}\n");

    // For Each Run ID: Scan the success / warning / error folders to fetch all Target Groups, like arm-01: https://github.com/lupyuen/nuttx-github-jobs
    let mut total_github_runner_minutes = 0;
    let mut total_adjusted_github_runner_minutes = 0;
    for run_id in &recent_jobs {
        let (target_groups, github_runner_minutes) = compute_github_runner_minutes(*run_id);
        println!("Run ID {run_id}: Target Groups: {target_groups:?}");
        println!("Run ID {run_id}: GitHub Runner Minutes: {github_runner_minutes}");
        if github_runner_minutes == 0 { continue; }
        
        // Inflate the minutes to account for missing jobs (Windows) and missing steps (Docker Pull)
        let adjusted_github_runner_minutes = (github_runner_minutes as f64 * 1.71) as u64;
        println!("Run ID {run_id}: Adjusted GitHub Runner Minutes: {adjusted_github_runner_minutes}");
        println!("Compare with https://github.com/apache/nuttx/actions/runs/{run_id}/usage");
        total_github_runner_minutes += github_runner_minutes;
        total_adjusted_github_runner_minutes += adjusted_github_runner_minutes;
    }
    println!("\nTotal GitHub Runner Minutes: {total_github_runner_minutes}");
    println!("Total Adjusted GitHub Runner Minutes: {total_adjusted_github_runner_minutes}");
}

/// Scan the Job-PR JSON for the Run IDs that were updated UTC 00:00 or later: https://github.com/lupyuen/nuttx-github-jobs/blob/main/nuttx-github-jobs.json
/// Return the Run ID Array and Jobs JSON Array.
/// Allow Multiple Run IDs for the same PR.
fn fetch_recent_jobs() -> (Vec<u64>, Vec<serde_json::Value>) {
    // TODO: Change to a File Stream Reader to avoid loading the entire JSON into memory
    // Open the Job-PR JSON File and create a Streaming JSON Reader
    let file = std::fs::read(JOB_PR_JSON).unwrap();
    let json_reader = SimpleJsonReader::new(file.as_slice());

    // For each Job-PR record in the array...
    let mut found_prs = Vec::<u64>::new();
    let mut recent_jobs = Vec::<u64>::new();

    json_reader.read_array_items(|array_reader| {
        // Fetch the Run ID, Updated At and PR Number:
        // {"job_updatedAt": "2026-04-01T22:06:23Z", "job_databaseId": 23873176516, "pr_number": 18654, ...
        let mut run_id = None::<u64>;
        let mut updated_at = None::<String>;
        let mut pr_number = None::<u64>;
        let mut job_name = None::<String>;
        array_reader.read_object_owned_names(|name, value_reader| {
            match name.as_str() {
                "job_databaseId" => {
                    let val: u64 = value_reader.read_number().unwrap().unwrap();
                    run_id = Some(val);
                },
                "pr_number" => {
                    let val: u64 = value_reader.read_number().unwrap().unwrap();
                    pr_number = Some(val);
                },
                "job_updatedAt" => {
                    let val: String = value_reader.read_string().unwrap();
                    updated_at = Some(val);
                },
                "job_name" => {
                    let val: String = value_reader.read_string().unwrap();
                    job_name = Some(val);
                },
                _ => {}
            }
            Ok(())
        })?;
        if run_id.is_none() || updated_at.is_none() || pr_number.is_none() || job_name.is_none() {
            return Err("Missing required fields".into());
        }
        let run_id = run_id.unwrap();
        let updated_at = updated_at.unwrap();
        let pr_number = pr_number.unwrap();
        let job_name = job_name.unwrap();
        if job_name != "Build" { return Ok(()); }

        // Stop if the Job-PR is not the same date as today
        let updated_at = chrono::DateTime::parse_from_rfc3339(&updated_at).unwrap();
        let now = chrono::Utc::now();
        if now.date_naive() != updated_at.date_naive() {
            println!("Stopping at Run ID {run_id} for PR#{pr_number} because it was updated at {updated_at} which is not today {now}");
            return Err("Not updated today".into());
        }

        // Remember the PR Number
        if !found_prs.contains(&pr_number) { found_prs.push(pr_number); }
        
        // Add the Job-PR to the Recent Jobs Array
        recent_jobs.push(run_id);
        Ok(())
    }).unwrap_or_default();

    // For each Recent Job-PR, Fetch the Job-PR JSON and add it to the Result Array
    let mut recent_jobs_json = Vec::<serde_json::Value>::new();
    for run_id in &recent_jobs {
        let job_pr = fetch_job_pr(*run_id);
        if let Ok(job_pr) = job_pr {
            // Ignore all Closed and Merged PRs
            let job_pr_value: serde_json::Value = serde_json::from_str(&job_pr).unwrap();
            recent_jobs_json.push(job_pr_value);
        }
    }
    (recent_jobs, recent_jobs_json)
}

/// For Each Run ID: Scan the success / warning / error folders to fetch all Target Groups,
/// like arm-01: https://github.com/lupyuen/nuttx-github-jobs
/// Return the Target Group Array for the Run ID, and the Total GitHub Runner Minutes for the Run ID.
fn compute_github_runner_minutes(run_id: u64) -> (Vec<String>, u64) {
    // Remember the Target Groups and the min / max timestamps for each Target Group
    let mut target_groups = HashSet::<String>::new();
    let mut target_group_timestamp_min = HashMap::<String, String>::new();
    let mut target_group_timestamp_max = HashMap::<String, String>::new();

    // Iterate through the Success, Error and Warning Folders for the Run ID
    for folder in ["success", "error", "warning"] {
        let path = format!("../nuttx-github-jobs/{folder}/{run_id}");
        if !std::path::Path::new(&path).exists() {
            println!("Skipping missing folder {path}");
            continue;
        }
        // Iterate through all filenames in the folder,
        // like arm-01:at32f437-mini:adc.json
        let entries: Vec<_> = read_dir(&path).unwrap().collect();
        for entry in entries.into_iter() {
            let entry = entry.unwrap();
            let path = entry.path();

            // Target Group is the first part of the filename: arm-01
            let filename = path.file_name().unwrap().to_str().unwrap();
            if filename.starts_with(".") { continue; }
            if !filename.contains(":") { println!("Skipping invalid filename {filename}"); continue; }
            if let Some(target_group) = filename.split(':').next() {
                if !target_groups.contains(target_group) {
                    target_groups.insert(target_group.to_string());
                }

                // For Each Run ID and Target Group:
                // Scan the success / warning / error folders
                // for the min and max timestamp
                let file = std::fs::read_to_string(path.clone()).unwrap();
                let json_value: serde_json::Value = serde_json::from_str(&file).unwrap();
                let timestamp = json_value["timestamp"].as_str().unwrap();
                if !target_group_timestamp_min.contains_key(target_group) || *timestamp < *target_group_timestamp_min[target_group] {
                    target_group_timestamp_min.insert(target_group.to_string(), timestamp.to_string());
                }
                if !target_group_timestamp_max.contains_key(target_group) || *timestamp > *target_group_timestamp_max[target_group] {
                    target_group_timestamp_max.insert(target_group.to_string(), timestamp.to_string());
                }
            }
        }
    }
    // Sort the Target Groups
    let mut target_groups = target_groups.into_iter().collect::<Vec<_>>();
    target_groups.sort();

    // Get the Min Timestamp, Max Timestamp and GitHub Runner Minutes for each Target Group
    let mut total_github_runner_minutes: u64 = 0;
    for target_group in &target_groups {
        let timestamp_min = target_group_timestamp_min.get(target_group).unwrap();
        let timestamp_max = target_group_timestamp_max.get(target_group).unwrap();
        let timestamp_min = chrono::DateTime::parse_from_rfc3339(&(timestamp_min.to_string() + "Z")).unwrap();
        let timestamp_max = chrono::DateTime::parse_from_rfc3339(&(timestamp_max.to_string() + "Z")).unwrap();
        let github_runner_minutes = (timestamp_max - timestamp_min).num_minutes() as u64;
        // println!("Run ID {run_id}: Target Group {target_group}: Min Timestamp: {timestamp_min}, Max Timestamp: {timestamp_max}, GitHub Runner Minutes: {github_runner_minutes}");
        total_github_runner_minutes += github_runner_minutes;
    }
    // println!("Run ID {run_id}: Total GitHub Runner Minutes: {total_github_runner_minutes}");
    (target_groups, total_github_runner_minutes)
}

/// Fetch the Job-PR JSON for a Given Run ID (Job ID)
fn fetch_job_pr(run_id: u64) -> Result<String, Box<dyn std::error::Error>> {
    // Open the Job-PR JSON File and create a Streaming JSON Reader
    let file = std::fs::read(JOB_PR_JSON)?;
    let json_reader = SimpleJsonReader::new(file.as_slice());

    // For each Job-PR record in the array...
    let mut index = Option::<usize>::None;
    let mut i = 0;
    json_reader.read_array_items(|array_reader| {
        // Fetch the Run ID: {"job_databaseId": 23688473202, ...
        array_reader.read_object_owned_names(|name, value_reader| {            
            // If the Run ID matches, remember the Found Index
            if name == "job_databaseId" {
                let val: u64 = value_reader.read_number().unwrap().unwrap();
                if val == run_id {
                    // We simulate an Error to quit early
                    index = Some(i);
                    println!("Found Job-PR Index: {i}");
                    return Err(format!("{i}").to_string().into());
                }
            }
            Ok(())
        })?;
        i += 1;
        Ok(())
    }).unwrap_or_default();

    // Quit if index not found
    if index.is_none() {
        println!("Run ID {run_id} not found in {JOB_PR_JSON}. Please regenerate the JSON File.");
        return Err("Run ID not found".into());
    }
    let index = index.unwrap() as u32;

    // Jump to the Found Index in the Job-PR array
    let file = std::fs::read(JOB_PR_JSON)?;
    let mut json_reader = JsonStreamReader::new(file.as_slice());
    let path = &json_path![index];
    json_reader.seek_to(path)?;

    // Extract the Job-PR
    let mut writer = Vec::<u8>::new();
    let mut json_writer = JsonStreamWriter::new(&mut writer);
    json_reader.transfer_to(&mut json_writer)?;
    json_writer.finish_document()?;
    let job_pr = String::from_utf8(writer)?;

    // Validate the Job-PR JSON with Serde
    let job_pr2: serde_json::Value = serde_json::from_str(&job_pr)?;
    let job_pr3 = serde_json::to_string_pretty(&job_pr2)?;
    Ok(job_pr3)
}
