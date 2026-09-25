#!/bin/bash

cd . # work on the current dir.

git add .
git commit -m "." || true
git fetch
git pull
git push
