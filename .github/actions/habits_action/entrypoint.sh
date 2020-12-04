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

# Run pipeline on args (args assigned in action.yaml)
# $1 - repo owner
# $2 - repo name
habits pipeline --mode "github_app" --owner $1 --repo $2

# Assign labels
habits label --owner $1 --repo $2