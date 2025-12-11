#!/usr/bin/env bash

# runs shellcheck and prints GitHub Actions annotations for each warning and error
# https://github.com/koalaman/shellcheck

find . -name "*.sh" -not -path "./.git/*" -exec shellcheck -f gcc {} + | \
  while IFS=: read -r file line col severity msg; do
    level="warning"
    [[ "$severity" =~ error ]] && level="error"
    echo "::${level} file=${file},line=${line},col=${col}::${msg}"
  done