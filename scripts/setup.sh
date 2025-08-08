#!/bin/bash
set -euo pipefail

echo "Creating /var/lib/noctiforge directory with proper permissions..."
sudo mkdir -pv /var/lib/noctiforge/registry
sudo chown -R "$(id -u):$(id -g)" /var/lib/noctifunc
