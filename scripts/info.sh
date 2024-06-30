#!/bin/bash

# Get current directory
output_file=$1

# Get Git commit message
git_commit_message=$(git log -1 --pretty=%B)

# Get Git hash
git_hash=$(git rev-parse HEAD)

# Get Git branch
git_branch=$(git rev-parse --abbrev-ref HEAD)

# Get Git remote URL
git_remote_url=$(git config --get remote.origin.url)

# Get current date
current_date=$(date +"%Y-%m-%d %T")


# Write information to the file
cat <<EOF > "$output_file"
Git Information
------------------
Date: $current_date
Git Commit Message: $git_commit_message
Git Hash: $git_hash
Git Branch: $git_branch
Git Remote URL: $git_remote_url
EOF
