# Repository-specific guidance

Keep rebuildable/disposable outputs under `target/`, with one build directory per profile. Retain small open-failure evidence under `target/evidence/<topic>/` and explicitly requested final reports/results/diffs; discard passing logs and archive copies. Follow [Contributing: build storage and cleanup](CONTRIBUTING.md#build-storage-and-cleanup), including cleanup of isolated outputs. Earlier/shared outputs require owner approval before deletion.
