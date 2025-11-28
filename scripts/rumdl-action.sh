#!/usr/bin/env bash

set -ou pipefail

rumdl_version="0.0.183"

echo
echo "Instaling rumdl (v$rumdl_version)"
pip install rumdl=="$rumdl_version"

echo
echo "Linting markdown with rumdl"

rumdl_output_format=""

case "$GHA_RUMDL_REPORT_TYPE" in
*"logs"*)
    rumdl_output_format="full"
    ;;
*"annotations"*)
    rumdl_output_format="github"
    ;;
*)
    echo
    echo "::error:: invalid report type : $GHA_RUMDL_REPORT_TYPE"
    echo "report type should be one of : logs, annotations"
    exit 1
    ;;
esac

results=$(rumdl check "$GITHUB_WORKSPACE" --output-format "$rumdl_output_format" 2>&1)

if [ $? -eq 0 ]; then
    echo "$results"
    exit 0
else
    case "$GHA_RUMDL_REPORT_TYPE" in
    *"logs"*)
        echo "$results"
        exit 1
        ;;
    *"annotations"*)
        # print each line of as github actions runtime annotation
        echo "$results" | grep '::' | xargs -0 -n1 echo

         # print what is left
        echo "$results" | grep -v '::'
        exit 1
        ;;
    esac
fi
