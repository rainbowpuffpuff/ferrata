#!/usr/bin/env bash
# Operate the Ferrata LEZ testnet on AWS from a workstation with the AWS CLI v2 (credentials from `aws login`),
# region us-east-1. Everything it creates is tagged Project=ferrata-testnet. No SSH: commands go over SSM.
#
#   deploy.sh stack [TYPE]       create the CloudFormation stack (stack.yaml), or update it keeping the AMI
#   deploy.sh bundle             this repository (git archive HEAD), the program binaries and, when given, the site
#                                bundle and RISC Zero's recursion_zkr.zip to the artifact bucket (bundle/)
#   deploy.sh provision          on the host: base, fetch, toolchain, build-zone, build-ferrata, build-explorer
#                                (provision.sh), detached; then `deploy.sh run install`
#   deploy.sh run STAGE...       one provision.sh stage on the host and its output (install, fetch, build-ferrata, ...)
#   deploy.sh programs STEP      deploy-programs.sh plan | fund | run | handover | publish | verify on the host
#                                (handover: the updater and attest-monitor under systemd; run it before `resize`)
#   deploy.sh resize TYPE        the steady-state instance type (stop/start; same address, volumes, state)
#   deploy.sh status             the stack's outputs and the network's status.json
#   deploy.sh sh 'COMMAND'       one root command on the host (SSM), its output
#
# PROGRAMS_DIR: the four program binaries (ferrata_lez.bin, ferrata_bridge_b1.bin, ferrata_holdings.bin,
# ferrata_consumer.bin), each checked against docs/image-ids.md before upload (scripts/reproduce-image-ids.sh builds them).
# WEB_TGZ (optional): the site bundle install-web.sh takes (README: Artifacts); fetched to /opt/ferrata/web.tar.gz.
# RECURSION_ZKR (optional): RISC Zero v3.0.5's src/recursion_zkr.zip (sha256 744b999f...), for build-explorer when
# RISC Zero's own URL refuses the host (it did: 403); provision passes the bucket to the stages as FERRATA_BUCKET.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
STACK="${STACK:-ferrata-testnet}"
export AWS_REGION="${AWS_REGION:-us-east-1}"
out() { aws cloudformation describe-stacks --stack-name "$STACK" --query "Stacks[0].Outputs[?OutputKey=='$1'].OutputValue" --output text; }
ssm() { # ssm 'commands' [timeout]: run as root on the host, print stdout/stderr
  local id cid
  id="$(out InstanceId)"
  cid="$(aws ssm send-command --instance-ids "$id" --document-name AWS-RunShellScript --timeout-seconds 600 \
      --parameters "$(python3 -I -c 'import json,sys; print(json.dumps({"commands":[sys.argv[1]],"executionTimeout":[sys.argv[2]]}))' "$1" "${2:-3600}")" \
      --query Command.CommandId --output text)"
  # Polled until it ends, up to its own timeout (AWS CLI's waiter gives up after 100 s; `programs run` takes ~2 h).
  local st deadline=$(( $(date +%s) + ${2:-3600} + 120 ))
  while :; do
    st="$(aws ssm get-command-invocation --command-id "$cid" --instance-id "$id" --query Status --output text 2>/dev/null || echo Pending)"
    case "$st" in Pending|InProgress|Delayed) ;; *) break ;; esac
    [ "$(date +%s)" -lt "$deadline" ] || break
    sleep 10
  done
  aws ssm get-command-invocation --command-id "$cid" --instance-id "$id" --query '[Status,StandardOutputContent,StandardErrorContent]' --output text
}

case "${1:-}" in
  stack)
    type="${2:-c7i.8xlarge}"
    if aws cloudformation describe-stacks --stack-name "$STACK" > /dev/null 2>&1; then
      aws cloudformation deploy --stack-name "$STACK" --template-file "$HERE/stack.yaml" --capabilities CAPABILITY_IAM \
        --parameter-overrides InstanceType="$type" AmiId="$(aws cloudformation describe-stacks --stack-name "$STACK" \
          --query "Stacks[0].Parameters[?ParameterKey=='AmiId'].ParameterValue" --output text)" --tags Project=ferrata-testnet
    else
      ami="$(aws ssm get-parameter --name /aws/service/canonical/ubuntu/server/24.04/stable/current/amd64/hvm/ebs-gp3/ami-id --query Parameter.Value --output text)"
      aws cloudformation deploy --stack-name "$STACK" --template-file "$HERE/stack.yaml" --capabilities CAPABILITY_IAM \
        --parameter-overrides InstanceType="$type" AmiId="$ami" --tags Project=ferrata-testnet
    fi
    aws cloudformation describe-stacks --stack-name "$STACK" --query 'Stacks[0].Outputs' --output table ;;
  bundle)
    bucket="$(out ArtifactBucketName)"; tmp="$(mktemp -d "${TMPDIR:-$REPO/..}/ferrata-bundle.XXXX")"
    git -C "$REPO" archive --format=tar.gz -o "$tmp/ferrata-src.tar.gz" HEAD
    P="${PROGRAMS_DIR:?PROGRAMS_DIR: the four program binaries}"
    # docs/image-ids.md: the four programs' sha256s (each file must have its own).
    ( cd "$P" && printf '%s  %s\n' \
        ddfde69bea89a5b882f55266fcb4b7c887df7debc1c9d4972d8ebea5a72eaad9 ferrata_lez.bin \
        537cbe7a4c6900419b54e556b9741a9e37d86adf53f5e65683aa2b22c8e9a036 ferrata_bridge_b1.bin \
        504a67c071a103b5d99de10163ae97f925151b09b59c49048e50f3d315b710c5 ferrata_holdings.bin \
        d02e4aa394f73702fec3e9983f32e8c9743dbaa1a41d2d705fc06f1b63e94ca5 ferrata_consumer.bin > SHA256SUMS \
      && sha256sum -c SHA256SUMS && tar -czf "$tmp/programs.tar.gz" ferrata_lez.bin ferrata_bridge_b1.bin \
         ferrata_holdings.bin ferrata_consumer.bin SHA256SUMS )
    aws s3 cp "$tmp/ferrata-src.tar.gz" "s3://$bucket/bundle/ferrata-src.tar.gz"
    aws s3 cp "$tmp/programs.tar.gz" "s3://$bucket/bundle/programs.tar.gz"
    if [ -n "${WEB_TGZ:-}" ]; then aws s3 cp "$WEB_TGZ" "s3://$bucket/bundle/web.tar.gz"; fi
    if [ -n "${RECURSION_ZKR:-}" ]; then
      echo "744b999f0a35b3c86753311c7efb2a0054be21727095cf105af6ee7d3f4d8849  $RECURSION_ZKR" | sha256sum -c - \
        && aws s3 cp "$RECURSION_ZKR" "s3://$bucket/bundle/recursion_zkr.zip"
    fi
    rm -rf "$tmp" ;;
  provision)
    bucket="$(out ArtifactBucketName)"; vol="$(out DataVolumeId)"
    ssm "set -e; command -v aws >/dev/null || snap install aws-cli --classic
mkdir -p /opt/ferrata/src/ferrata /var/log/ferrata; aws s3 cp --only-show-errors s3://$bucket/bundle/ferrata-src.tar.gz /opt/ferrata/src.tgz
tar -xzf /opt/ferrata/src.tgz -C /opt/ferrata/src/ferrata; install -m 755 /opt/ferrata/src/ferrata/deploy/testnet/provision.sh /opt/ferrata/provision.sh
systemd-run --unit=ferrata-chain --collect -E FERRATA_BUCKET=$bucket /bin/bash -c 'P=/opt/ferrata/provision.sh; for s in \"base $vol\" \"fetch $bucket\" toolchain build-zone build-ferrata build-explorer; do \$P \$s || exit 1; done'
echo started" 120 ;;
  run) shift; ssm "/opt/ferrata/provision.sh fetch $(out ArtifactBucketName) >/dev/null && for s in $*; do /opt/ferrata/src/ferrata/deploy/testnet/provision.sh \$s 2>&1 | tail -n 40; done" ;;
  programs) ssm "/opt/ferrata/src/ferrata/deploy/testnet/deploy-programs.sh ${2:?plan|fund|run|handover|publish|verify}" 7200 ;;
  resize) "$0" stack "${2:?instance type}" ;;
  status)
    aws cloudformation describe-stacks --stack-name "$STACK" --query 'Stacks[0].Outputs' --output table
    curl -fsS "https://$(out Hostname)/status/status.json" | python3 -I -m json.tool | head -n 60 ;;
  sh) ssm "${2:?command}" ;;
  *) sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
