# Updated GitHub Runner Metrics for Apache NuttX Project

Computed based on Downloaded GitHub Job Status. See https://github.com/apache/nuttx/issues/18906

```bash
pushd ../nuttx-github-jobs ; git pull ; popd ; cargo run
```

1. Scan the Job-PR JSON for the Run IDs that were updated UTC 00:00 or later: https://github.com/lupyuen/nuttx-github-jobs/blob/main/nuttx-github-jobs.json

1. For Each Run ID: Scan the success / warning / error folders to fetch all Target Groups, like arm-01: https://github.com/lupyuen/nuttx-github-jobs

1. For Each Run ID and Target Group: Scan the success / warning / error folders for the min and max timestamp.

1. For Each Run ID and Target Group: Compute the GitHub Runner Minutes based on max timestamp - min timestamp.

1. For Each Run ID: Add up the GitHub Runner Minutes for all Target Groups.

1. Add up all Target Groups to get the Total GitHub Runner Minutes since UTC 00:00.
