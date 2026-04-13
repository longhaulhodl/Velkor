#!/bin/bash
# ---------------------------------------------------------------------------
# Wait for MinIO and create the default bucket
# ---------------------------------------------------------------------------
set -e

BUCKET="${S3_BUCKET:-velkor-documents}"

echo "Waiting for MinIO..."
until curl -sf http://minio:9000/minio/health/live > /dev/null 2>&1; do
  sleep 1
done
echo "MinIO is ready."

# Configure mc client
mc alias set local http://minio:9000 "${MINIO_ROOT_USER}" "${MINIO_ROOT_PASSWORD}"

# Create bucket if it doesn't exist
if ! mc ls "local/${BUCKET}" > /dev/null 2>&1; then
  mc mb "local/${BUCKET}"
  echo "Bucket '${BUCKET}' created."
else
  echo "Bucket '${BUCKET}' already exists."
fi
