#!/bin/bash
set -e

echo "🔨 Building..."
docker run --rm --platform linux/amd64 \
  -v $(pwd):/app \
  -w /app \
  rust:latest \
  bash -c "cargo install dioxus-cli --locked && dx bundle --release --platform web"

echo "📦 Copying to server..."
scp target/dx/peeplist/release/web/server root@134.209.212.174:~/dev/peeplist/
scp -r target/dx/peeplist/release/web/public root@134.209.212.174:~/dev/peeplist/

echo "🚀 Restarting server..."
ssh root@134.209.212.174 "cd ~/dev/peeplist && pkill server 2>/dev/null; nohup bash up.sh > server.log 2>&1 &"

echo "✅ Done! Visit http://134.209.212.174:8080"
