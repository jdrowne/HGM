#!/bin/bash
set -euxo pipefail
source /opt/miniconda3/bin/activate
conda create -n testbed python=3.11  -y
conda activate testbed
conda clean -afy
