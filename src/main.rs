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

    // // Merge the Job-PR JSON with the Build JSON for each Run ID (Job ID) in the Error and Warning Folders
    // let merged_json_array = merge_job_pr_with_build();

    // // Count the number of Builds for each PR in the Recent Jobs
    // let pr_build_counts = count_pr_builds(&recent_jobs);
    // println!("PR Build Counts: {pr_build_counts:?}\n");

    // // Inject the Build Counts into the Recent Jobs JSON
    // let mut recent_jobs_with_counts = recent_jobs.clone();
    // for job_pr in recent_jobs_with_counts.as_array_mut().unwrap() {
    //     let pr_number = job_pr["pr_number"].as_u64().unwrap_or_default();
    //     let build_count = pr_build_counts.get(&pr_number).cloned().unwrap_or(0);
    //     job_pr.as_object_mut().unwrap().insert("build_count".to_string(), serde_json::Value::Number(build_count.into()));
    // }

    // // Render the Recent Jobs as HTML Table
    // let recent_jobs_html = render_recent_jobs(&recent_jobs_with_counts);
    // println!("Recent Jobs HTML:\n{recent_jobs_html}\n");

    // // Write the Recent Jobs JSON and the Merged Job-PR-Build JSON to Static Files
    // let merged_json_array_str = serde_json::to_string_pretty(&merged_json_array).unwrap();
    // std::fs::write("../nuttx-github-jobs/build-monitor.json", merged_json_array_str).unwrap();
    // let recent_jobs_json_str = serde_json::to_string_pretty(&recent_jobs_with_counts).unwrap();
    // std::fs::write("../nuttx-github-jobs/build-monitor-pr.json", recent_jobs_json_str).unwrap();

    // // Generate the HTML Table from Merged Job-PR-Build JSON
    // let table = render_job_pr_build(&merged_json_array);
    // let html = html_header(&recent_jobs_html) + 
    //     &table.to_html_string() +
    //     html_footer();
    // println!("html:\n{html}");

    // // Write the HTML Table to a Static File
    // std::fs::write("../nuttx-github-jobs/build-monitor.html", html).unwrap()
}

/// Scan the Job-PR JSON for the Run IDs that were updated UTC 00:00 or later.
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
            let pr_state = job_pr_value["pr_state"].as_str().unwrap_or_default();
            if pr_state == "CLOSED" || pr_state == "MERGED" { continue; }
            recent_jobs_json.push(job_pr_value);
        }
    }
    (recent_jobs, recent_jobs_json)
}

/// Render the Recent Jobs as HTML Table
fn render_recent_jobs(recent_jobs: &serde_json::Value) -> String {
    let mut table = Table::new()
        .with_attributes([("class", "w-full text-left border-collapse table-fixed min-w-[1200px]")]);
    let mut row = TableRow::new()
        .with_attributes([("class", "text-xs uppercase tracking-wider text-gray-500 font-semibold")]);
    for job_pr in recent_jobs.as_array().unwrap() {
        let pr_number = job_pr["pr_number"].as_u64().unwrap_or_default();
        let pr_url = job_pr["pr_url"].as_str().unwrap_or_default();
        let mut pr_title = job_pr["pr_title"].as_str().unwrap_or_default().to_string();
        let job_conclusion = job_pr["job_conclusion"].as_str().unwrap_or_default();
        let started_at = job_pr["job_startedAt"].as_str().unwrap_or_default();
        let updated_at = job_pr["job_updatedAt"].as_str().unwrap_or_default();
        let build_count = job_pr["build_count"].as_u64().unwrap_or_default();

        // Compute the Elapsed Time since the Job was Started: HH:MM:SS
        let started_at = chrono::DateTime::parse_from_rfc3339(started_at).unwrap();
        let updated_at = // If the Job is still running: Compute the Elapsed Time based on the current time
            if job_conclusion.is_empty() { chrono::Utc::now() }
            else { chrono::DateTime::parse_from_rfc3339(updated_at).unwrap().with_timezone(&chrono::Utc) };
        let elapsed = updated_at.signed_duration_since(started_at);
        let hours = elapsed.num_hours();
        let minutes = elapsed.num_minutes() % 60;
        let elapsed_str = format!("{hours}h {minutes}m");

        // If the Job is still running: Compute the Elapsed Time based on the current time
        let elapsed_str = if job_conclusion.is_empty() {
            let now = chrono::Utc::now();
            let elapsed = now.signed_duration_since(started_at);
            let hours = elapsed.num_hours();
            let minutes = elapsed.num_minutes() % 60;
            format!("{hours}h {minutes}m")
        } else {
            elapsed_str
        };

        // Choose an Icon based on the Job Conclusion
        let icon = match job_conclusion {
            "" => "loader",  // Still running
            _ =>  "clock"
        };
        let icon = // Flag any slow jobs
            if elapsed.num_minutes() > 4 * 60 { "alert-triangle" } 
            else { icon };

        // Colour the PR based on the Job Conclusion
        let pr_attr = match job_conclusion {
            "" => "bg-orange-600",  // Still running
            "action_required" => "bg-blue-900",
            "cancelled" => "bg-purple-900",
            "failure" => "bg-red-900",
            "startup_failure" => "bg-cyan-900",
            "success" => "bg-green-900",
            _ => "bg-slate-900"
        }; 
        let pr_attr = format!("{pr_attr} align-top p-4 text-slate-200 hover:text-slate-100 transition-colors border-r border-white/10 last:border-0");

        // Warn if too many builds
        let build_count_msg = 
            if build_count >= 10 { format!(r#"<span class="text-[10px] truncate opacity-80 flex items-center mt-1"><i data-lucide="alert-triangle" class="w-4 h-4 mr-1"></i> {build_count} Builds</span>"#) }
            else { "".to_string() };

        // Compose the PR Text
        pr_title.truncate(50);
        let pr_text = format!(r#"
            <span class="opacity-80 flex items-center mb-1"><i data-lucide="{icon}" class="w-4 h-4 mr-1"></i> {elapsed_str}</span>
            <span class="font-bold block">PR#{pr_number}</span>
            <span class="block truncate mt-1">{pr_title}</span>
            {build_count_msg}
        "#);
        row.add_cell(TableCell::default()
            .with_attributes([("class", pr_attr.as_str())])
            .with_link(pr_url, pr_text)
        );
    }
    table.add_custom_body_row(row);
    table.to_html_string()
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

/// Count the number of Builds for each PR in the Recent Jobs
fn count_pr_builds(recent_jobs: &serde_json::Value) -> HashMap<u64, usize> {
    let mut pr_build_counts = HashMap::<u64, usize>::new();
    for job_pr in recent_jobs.as_array().unwrap() {
        let pr_number = job_pr["pr_number"].as_u64().unwrap_or_default();
        pr_build_counts.insert(pr_number, 0);
    }

    // Scan the Job-PR JSON and count the number of Builds for each PR in the Recent Jobs
    let file = std::fs::read(JOB_PR_JSON).unwrap();
    let json_reader = SimpleJsonReader::new(file.as_slice());
    json_reader.read_array_items(|array_reader| {
        let mut pr_number = None::<u64>;
        let mut started_at = None::<String>;
        let mut job_name = None::<String>;
        array_reader.read_object_owned_names(|name, value_reader| {
            match name.as_str() {
                "pr_number" => {
                    let val: u64 = value_reader.read_number().unwrap().unwrap();
                    pr_number = Some(val);
                }
                "job_startedAt" => {
                    let val: String = value_reader.read_string().unwrap();
                    started_at = Some(val);
                },
                "job_name" => {
                    let val: String = value_reader.read_string().unwrap();
                    job_name = Some(val);
                },
                _ => {}
            }
            Ok(())
        })?;
        if pr_number.is_none() || started_at.is_none() || job_name.is_none() {
            return Err("Missing required fields".into());
        }
        let pr_number = pr_number.unwrap();
        let started_at = started_at.unwrap();
        let job_name = job_name.unwrap();
        if job_name != "Build" { return Ok(()); }
        if pr_build_counts.contains_key(&pr_number) {
            *pr_build_counts.get_mut(&pr_number).unwrap() += 1;
        }
        // Quit if the Job-PR is Older than 30 days
        if started_at.parse::<chrono::DateTime<chrono::Utc>>().unwrap_or_else(|_| chrono::Utc::now()) < chrono::Utc::now() - chrono::Duration::days(30) {
            return Err("Older than 30 days".into());
        }
        Ok(())
    }).unwrap_or_default();
    pr_build_counts
}

/// Merge the Build JSON into the Job-PR JSON for a Given Run ID (Job ID)
fn merge_build_json(build_json_path: &str, job_pr: &str) -> Result<String, Box<dyn std::error::Error>> {
    let build_json = std::fs::read_to_string(build_json_path)?;
    let mut job_pr_value: serde_json::Value = serde_json::from_str(job_pr)?;
    let build_value: serde_json::Value = serde_json::from_str(&build_json)?;

    // Merge the Build JSON into the Job-PR JSON
    if let serde_json::Value::Object(ref mut job_pr_map) = job_pr_value
        && let serde_json::Value::Object(build_map) = build_value {
        for (key, value) in build_map {
            let key = format!("build_{key}")
                .replace("build_build_", "build_");
            job_pr_map.insert(key, value);
        }
    }
    let merged_json = serde_json::to_string_pretty(&job_pr_value)?;
    Ok(merged_json)
}

/// Generate the HTML Header
fn html_header(recent_jobs_html: &str) -> String {
    let now = &chrono::Utc::now().to_rfc3339()[..19].replace("T", " ");
    format!(
r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>NuttX Build Monitor</title>
    <!-- Import Tailwind CSS for styling -->
    <script src="https://cdn.tailwindcss.com"></script>
    <!-- Import Lucide Icons for some visual flair -->
    <script src="https://unpkg.com/lucide@latest"></script>
    <style>
        /* Custom scrollbar for better visibility on horizontal tables */
        .custom-scrollbar::-webkit-scrollbar {{
            height: 6px;
        }}
        .custom-scrollbar::-webkit-scrollbar-track {{
            background: #f1f1f1;
        }}
        .custom-scrollbar::-webkit-scrollbar-thumb {{
            background: #cbd5e1;
            border-radius: 10px;
        }}
        .custom-scrollbar::-webkit-scrollbar-thumb:hover {{
            background: #94a3b8;
        }}
    </style>
</head>
<body class="bg-gray-50 text-gray-800 p-4 md:p-8 font-sans antialiased">

    <div class="w-full mx-auto">

        <!-- Dashboard Header Begin -->
        <div class="mb-6 flex flex-col md:flex-row md:items-center justify-between gap-4">
            <div>
                <h1 class="text-2xl font-bold text-gray-900 flex items-center gap-2">
                    <i data-lucide="activity" class="text-blue-600"></i>
                    NuttX Build Monitor
                    <a href="https://github.com/apache/nuttx/issues/18659">
                        <i data-lucide="info"></i>
                    </a>
                </h1>
                <p class="text-sm text-gray-500 mt-1">Recent errors and warnings for NuttX GitHub CI</p>
            </div>
            <div class="text-sm text-gray-500 bg-white px-4 py-2 rounded-full border border-gray-200 shadow-sm flex items-center gap-2">
                <i data-lucide="clock" class="w-4 h-4"></i>
                Updated: {now} UTC
            </div>
        </div>
        <!-- Dashboard Header End -->

        <!-- Recent Jobs Table Begin -->
        <div class="bg-white rounded-xl shadow-sm border border-gray-200 overflow-hidden mb-8">
            <!-- Responsive wrapper to prevent breaking on small screens -->
            <div class="overflow-x-auto custom-scrollbar">
                {recent_jobs_html}
            </div>
        </div>
        <!-- Recent Jobs Table End -->

        <!-- Table Card Begin -->
        <div class="bg-white rounded-xl shadow-sm border border-gray-200 overflow-hidden">
            <!-- Responsive wrapper to prevent breaking on small screens -->
            <div class="overflow-x-auto custom-scrollbar">
"#)
}

/// Generate the HTML Footer
fn html_footer() -> &'static str {
r#"
            </div>
        </div>
        <!-- Table Card End -->
    </div>

    <!-- Initialize icons and filters -->
    <script>
        // TODO: Handle the "(Same)" rows

        window.onload = function() {
            lucide.createIcons();
            setupFilters();
        };

        function setupFilters() {
            const tableBody = document.getElementById('error-table-body');
            const rows = Array.from(tableBody.querySelectorAll('tr'));
            
            // Identify unique PRs and Boards
            const timestamps = new Set();
            const prs = new Set();
            const boards = new Set();
            const errorWarnings = new Set();

            rows.forEach(row => {
                if (row.cells && row.cells.length > 2) {
                    const timestampCellText = row.cells[0].innerText.split('\n')[0]; // Extract Timestamp
                    const prCellText = row.cells[1].innerText.split('\n').join('').split(':')[0]; // Extract PR Number
                    const boardCellText = row.cells[2].innerText.split('\n').join('').split(':')[0]; // Extract Board Name
                    const errorWarningClass = row.cells[3].classList; // Extract class "error" or "warning"
                    const errorWarningCellText =
                        errorWarningClass.contains('error') ? 'Error' :
                        errorWarningClass.contains('warning') ? 'Warning' :
                        '(Unknown)';
                    
                    if (timestampCellText) timestamps.add(timestampCellText);
                    if (prCellText) prs.add(prCellText);
                    if (boardCellText) boards.add(boardCellText);
                    if (errorWarningCellText) errorWarnings.add(errorWarningCellText);
                }
            });

            // Setup Timestamp Filter Dropdown
            const timestampHeader = document.getElementById('filter-timestamp');
            const timestampSelect = createSelect('Filter...', timestamps, false);
            timestampHeader.innerHTML = '<div class="mb-1">Timestamp</div>';
            timestampHeader.appendChild(timestampSelect);

            // Setup PR Filter Dropdown
            const prHeader = document.getElementById('filter-pr');
            const prSelect = createSelect('Filter...', prs, false);
            prHeader.innerHTML = '<div class="mb-1">Pull Request</div>';
            prHeader.appendChild(prSelect);

            // Setup Board Filter Dropdown
            const boardHeader = document.getElementById('filter-board');
            const boardSelect = createSelect('Filter...', boards, true);
            boardHeader.innerHTML = '<div class="mb-1">Board / Config</div>';
            boardHeader.appendChild(boardSelect);

            // Setup Error / Warning Filter Dropdown
            const errorWarningHeader = document.getElementById('filter-error-warning');
            const errorWarningSelect = createSelect('Filter...', errorWarnings, true);
            errorWarningHeader.innerHTML = '<div class="mb-1">Error / Warning</div>';
            errorWarningHeader.appendChild(errorWarningSelect);

            // Add Event Listeners
            timestampSelect.addEventListener('change', () => filterRows(rows, timestampSelect.value, prSelect.value, boardSelect.value, errorWarningSelect.value));
            prSelect.addEventListener('change', () => filterRows(rows, timestampSelect.value, prSelect.value, boardSelect.value, errorWarningSelect.value));
            boardSelect.addEventListener('change', () => filterRows(rows, timestampSelect.value, prSelect.value, boardSelect.value, errorWarningSelect.value));
            errorWarningSelect.addEventListener('change', () => filterRows(rows, timestampSelect.value, prSelect.value, boardSelect.value, errorWarningSelect.value));
        }

        function createSelect(placeholder, values, ascending) {
            const select = document.createElement('select');
            select.className = 'filter-select';
            
            const defaultOption = document.createElement('option');
            defaultOption.value = 'all';
            defaultOption.innerText = placeholder;
            select.appendChild(defaultOption);

            // Sort by PR Number in descending order and Boards alphabetically
            Array.from(values).sort((a, b) => {
                if (ascending) { return a.localeCompare(b, undefined, { numeric: true }); }
                else { return b.localeCompare(a, undefined, { numeric: true }); }
            }).forEach(val => {
                const opt = document.createElement('option');
                opt.value = val;
                opt.innerText = val;
                select.appendChild(opt);
            });

            return select;
        }

        function filterRows(rows, timestampValue, prValue, boardValue, errorWarningValue) {
            rows.forEach(row => {
                if (row.cells && row.cells.length > 2) {
                    const rowTimestamp = row.cells[0].innerText.split('\n')[0]; // Extract Timestamp
                    const rowPr = row.cells[1].innerText.split('\n').join('').split(':')[0]; // Extract PR Number
                    const rowBoard = row.cells[2].innerText.split('\n').join('').split(':')[0]; // Extract Board Name
                    const rowErrorWarningClass = row.cells[3].classList; // Extract class "error" or "warning"
                    const rowErrorWarning =
                        rowErrorWarningClass.contains('error') ? 'Error' :
                        rowErrorWarningClass.contains('warning') ? 'Warning' :
                        '(Unknown)';

                    const matchesTimestamp = timestampValue === 'all' || rowTimestamp === timestampValue;
                    const matchesPr = prValue === 'all' || rowPr === prValue;
                    const matchesBoard = boardValue === 'all' || rowBoard === boardValue;
                    const matchesErrorWarning = errorWarningValue === 'all' || rowErrorWarning === errorWarningValue;

                    if (matchesTimestamp && matchesPr && matchesBoard && matchesErrorWarning) {
                        row.classList.remove('hidden');
                        row.style.opacity = '1';
                    } else {
                        row.classList.add('hidden');
                    }
                }
            });
        }
    </script>

</body>
</html>
"#
}