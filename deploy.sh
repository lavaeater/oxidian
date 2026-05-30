#!/usr/bin/env bash
set -euo pipefail

# === CONFIGURATION ===
       # Replace with your Amplify App ID
BRANCH_NAME="main"              # The Amplify branch you want to deploy to
ZIP_FILE="deploy.zip"
POLL_INTERVAL=5                # Seconds between status checks

# === ARGUMENT PARSING ===
if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <app-id> <folder-to-zip>"
  exit 1
fi

APP_ID="$1"

FOLDER="$2"

if [[ ! -d "$FOLDER" ]]; then
  echo "Error: folder '$FOLDER' does not exist"
  exit 1
fi

# === ZOLA Build === #
zola build

# === PACKAGE DIRECTORY ===
echo "📦 Zipping contents of '$FOLDER'..."
rm -f "$ZIP_FILE"
(
  cd "$FOLDER"
  zip -r "../$ZIP_FILE" . -x "*.git*" -x "../$ZIP_FILE"
)

# === CREATE DEPLOYMENT ===
echo "🚀 Creating new deployment in Amplify..."
DEPLOYMENT=$(aws amplify create-deployment --app-id "$APP_ID" --branch-name "$BRANCH_NAME")
UPLOAD_URL=$(echo "$DEPLOYMENT" | jq -r '.zipUploadUrl')
JOB_ID=$(echo "$DEPLOYMENT" | jq -r '.jobId')

# === UPLOAD FILE ===
echo "⬆️ Uploading build to Amplify (via pre-signed S3 URL)..."
curl -s -T "$ZIP_FILE" "$UPLOAD_URL"

# === START DEPLOYMENT ===
echo "▶️ Starting deployment job..."
aws amplify start-deployment \
  --app-id "$APP_ID" \
  --branch-name "$BRANCH_NAME" \
  --job-id "$JOB_ID" >/dev/null

# === POLL STATUS ===
echo "⏳ Waiting for deployment to complete..."
while true; do
    JOB_INFO=$(aws amplify get-job --app-id "$APP_ID" --branch-name "$BRANCH_NAME" --job-id "$JOB_ID")
    STATUS=$(echo "$JOB_INFO" | jq -r '.job.summary.status')

    echo "   Current status: $STATUS"

    if [[ "$STATUS" == "SUCCEED" ]]; then
        echo "✅ Deployment completed successfully!"
        rm "$ZIP_FILE"
        break
    elif [[ "$STATUS" == "FAILED" || "$STATUS" == "CANCELLED" ]]; then
        echo "❌ Deployment failed!"
        echo "$JOB_INFO" | jq -r '.job'
        exit 1
    fi

    sleep "$POLL_INTERVAL"
done

# aws amplify stop-job app-id d2aomkdti08kxf --branch-name main --job-id $JOB_ID