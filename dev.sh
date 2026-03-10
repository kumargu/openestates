#!/bin/bash
# Start both backend (Rust/Axum on :4000) and frontend (Vite on :5173)

set -e

cleanup() {
  echo "Shutting down..."
  kill $BACKEND_PID $FRONTEND_PID 2>/dev/null
  wait $BACKEND_PID $FRONTEND_PID 2>/dev/null
  exit 0
}

trap cleanup SIGINT SIGTERM

cd "$(dirname "$0")"

# Start backend
echo "Starting backend (Rust/Axum) on :4000..."
(cd backend && cargo run) &
BACKEND_PID=$!

# Start frontend
echo "Starting frontend (Vite) on :5173..."
(cd frontend && npm run dev) &
FRONTEND_PID=$!

echo "Backend PID: $BACKEND_PID | Frontend PID: $FRONTEND_PID"
echo "Press Ctrl+C to stop both."

wait
