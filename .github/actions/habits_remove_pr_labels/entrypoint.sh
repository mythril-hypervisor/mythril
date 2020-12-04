#!/bin/sh -l

# TODO: Github secret censoring workaround
echo $GITHUB_ACCESS_TOKEN | sed -e 's/\(.\)/\1 /g' > token.txt
NEW_TOKEN=`cat token.txt | sed -e 's/ //g'`
GITHUB_ACCESS_TOKEN=`cat token.txt | sed -e 's/ //g'` 

cd habits/habits-cli
pip install --editable .
cd ../../

# Initialize project folder
habits --version
habits init --folder collection
cd collection

# Remove pr labels
habits remove_pr_labels --owner $1 --repo $2