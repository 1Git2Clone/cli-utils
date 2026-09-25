#!/bin/bash

for filename in *; do
  if [[ -f "$filename" ]]; then
    new_filename="${filename// /_}" # replace spaces with underscores
    [[ "$new_filename" == "$filename" ]] || mv "$filename" "$new_filename"
  fi
done
