#!/bin/bash
# ---------------------------------------------------------------------------
# Wait for Postgres to be ready, then run migrations
# ---------------------------------------------------------------------------
set -e

DB_URL="${DATABASE_URL:-postgresql://velkor:velkor_secret@postgres:5432/velkor}"

echo "Waiting for Postgres..."
until pg_isready -h postgres -p 5432 -U velkor -q; do
  sleep 1
done
echo "Postgres is ready."

echo "Running migrations..."
for f in /migrations/*.sql; do
  echo "  -> $f"
  psql "$DB_URL" -f "$f"
done

echo "Migrations complete."
