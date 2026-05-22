//! Scan the Job-PR JSON for the Run IDs that were updated UTC 00:00 or later: https://github.com/lupyuen/nuttx-github-jobs/blob/main/nuttx-github-jobs.json
//! For Each Run ID: Scan the success / warning / error folders to fetch all Target Groups, like arm-01: https://github.com/lupyuen/nuttx-github-jobs
//! For Each Run ID and Target Group: Scan the success / warning / error folders for the min and max timestamp.
//! For Each Run ID and Target Group: Compute the GitHub Runner Minutes based on max timestamp - min timestamp.
//! For Each Run ID: Add up the GitHub Runner Minutes for all Target Groups.
//! Add up all Target Groups to get the Total GitHub Runner Minutes since UTC 00:00.
use std::{collections::HashMap, fs::read_dir, thread::sleep, time::Duration};
use build_html::{Html, HtmlContainer, Table, TableCell, TableCellType, TableRow};
use struson::{
    json_path,
    reader::{JsonReader, JsonStreamReader, simple::{SimpleJsonReader, ValueReader}},
    writer::{JsonStreamWriter, JsonWriter}
};

/// JSON File that contains the Job-PR records for all NuttX GitHub Jobs
const JOB_PR_JSON: &str = "../nuttx-github-jobs/nuttx-github-jobs.json";

fn main() {
    // Fetch the Recent Jobs from the Job-PR JSON
    let (recent_jobs, recent_jobs_json) = fetch_recent_jobs();
    println!("Recent Jobs: {recent_jobs:?}\n");
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

    found_prs.push(18933); //// TODO: Remove
    recent_jobs.push(26231721423); //// TODO: Remove

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

/// For Each Run ID: Scan the success / warning / error folders to fetch all Target Groups, like arm-01: https://github.com/lupyuen/nuttx-github-jobs
// fn merge_job_pr_with_build() -> Vec<serde_json::Value> {
//     // Remember the Merged Job-PR-Build JSON for each Run ID
//     let mut merged_json_array = Vec::<serde_json::Value>::new();

//     // Iterate through the Error and Warning Folders
//     for folder in ["error", "warning"] {
//         let path = format!("../nuttx-github-jobs/{folder}");
//         if !std::path::Path::new(&path).exists() {
//             println!("Folder {path} does not exist. Please parse-nuttx-builds first.");
//             return merged_json_array;
//         }

//         // Iterate Backwards through all Run IDs (Job IDs) in the Error and Warning Folders
//         // Like ../nuttx-github-jobs/error/23712816820
//         let mut entries: Vec<_> = read_dir(&path).unwrap().collect();
//         entries.sort_by_key(|entry| entry.as_ref().unwrap().path());
//         for entry in entries.into_iter().rev() {
//             let entry = entry.unwrap();
//             let path = entry.path();
//             println!("Found Build Path: {path:?}");

//             // Run ID is the last part of the path: 23712816820
//             let run_id = path.file_name().unwrap().to_str().unwrap();
//             if run_id.starts_with(".") { continue; }
//             let run_id = run_id.parse::<u64>();
//             let run_id = match run_id {
//                 Ok(id) => id,
//                 Err(e) => {
//                     println!("Skipping invalid Run ID: {e}");
//                     sleep(Duration::from_secs(1));
//                     continue;
//                 }
//             };
//             println!("Run ID: {run_id}");

//             // For each Run ID (Job ID), Fetch the Job-PR JSON
//             let job_pr = fetch_job_pr(run_id);
//             let job_pr = match job_pr {
//                 Ok(json) => json,
//                 Err(e) => {
//                     println!("Error fetching Job-PR JSON: {e}");
//                     sleep(Duration::from_secs(1));
//                     continue;
//                 }
//             };

//             // Stop iterating when Job Timestamp is too old
//             let job_pr_json: serde_json::Value = serde_json::from_str(&job_pr).unwrap();
//             let timestamp = job_pr_json["job_startedAt"].as_str().unwrap();
//             let timestamp = chrono::DateTime::parse_from_rfc3339(timestamp).unwrap();
//             if timestamp < chrono::Utc::now() - chrono::Duration::days(28) {
//                 println!("Build is too old. Stopping iteration for folder {folder}");
//                 break;
//             }

//             // Generate the Merged Job-PR-Build JSON for each Run ID:
//             // Iterate through all Build JSON files in the folder
//             // Like ../nuttx-github-jobs/error/23712816820/xtensa-03:lckfb-szpi-esp32s3:uvc.json
//             let entries: Vec<_> = read_dir(&path).unwrap().collect();
//             for entry in entries.into_iter() {
//                 let entry = entry.unwrap();
//                 let path = entry.path().to_str().unwrap().to_string();
//                 println!("Found Build JSON: {path}");

//                 // Merge the Build JSON into the Job-PR JSON
//                 let merged_json = merge_build_json(&path, &job_pr);
//                 let merged_json = match merged_json {
//                     Ok(json) => json,
//                     Err(e) => {
//                         println!("Error merging Build JSON: {e}");
//                         sleep(Duration::from_secs(1));
//                         continue;
//                     }
//                 };
//                 println!("merged_json:\n{merged_json}\n");

//                 // Add the Merged JSON into a JSON Array
//                 let merged_json_value: serde_json::Value = serde_json::from_str(&merged_json).unwrap();
//                 merged_json_array.push(merged_json_value.clone());
//             }
//         }
//     }

//     // Sort the JSON Array by Timestamp in Descending Order (Latest First)
//     merged_json_array.sort_by(|a, b| {
//         let a_timestamp = a["build_timestamp"].as_str().unwrap_or_default();
//         let b_timestamp = b["build_timestamp"].as_str().unwrap_or_default();
//         b_timestamp.cmp(a_timestamp)
//     });    
//     merged_json_array
// }

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
