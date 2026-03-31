# Terraform Infrastructure

This directory contains all Terraform configuration for the Axionvera Network infrastructure.

## Remote State Backend

State is stored remotely in AWS S3 with DynamoDB state locking to prevent concurrent `apply` operations.

| Resource | Name |
|---|---|
| S3 bucket | `axionvera-network-terraform-state` |
| DynamoDB table | `axionvera-network-terraform-locks` |
| Region | `us-east-1` |

The S3 bucket has **SSE-S3 encryption** (`AES256`) and **versioning** enabled, allowing state rollback if needed.

## First-Time Backend Initialisation

The S3 bucket and DynamoDB table must exist before the remote backend can be used. Bootstrap them once using a temporary local backend:

**1. Comment out the `backend "s3"` block in `main.tf`:**

```hcl
terraform {
  required_version = ">= 1.5.0"
  # backend "s3" { ... }   <-- comment this out temporarily
  ...
}
```

**2. Apply just the backend bootstrap resources:**

```bash
cd terraform
terraform init
terraform apply -target=aws_s3_bucket.terraform_state \
                -target=aws_s3_bucket_versioning.terraform_state \
                -target=aws_s3_bucket_server_side_encryption_configuration.terraform_state \
                -target=aws_s3_bucket_public_access_block.terraform_state \
                -target=aws_dynamodb_table.terraform_locks
```

**3. Restore the `backend "s3"` block, then re-initialise and migrate state:**

```bash
terraform init -migrate-state
```

Terraform will prompt you to copy the existing local state into the S3 bucket. Confirm with `yes`.

**4. Verify the backend is active:**

```bash
terraform state list
```

## Day-to-Day Usage

```bash
cd terraform
terraform init      # only needed after a fresh clone or backend change
terraform plan
terraform apply
```

## State Locking

Every `plan` and `apply` acquires a lock in the DynamoDB table. If a run is interrupted and the lock is not released automatically, remove it with:

```bash
terraform force-unlock <LOCK_ID>
```

The lock ID is printed in the error message when a lock conflict occurs.

## State Rollback

Because versioning is enabled on the S3 bucket, you can restore a previous state file from the AWS Console or CLI:

```bash
aws s3api list-object-versions \
  --bucket axionvera-network-terraform-state \
  --prefix network-infrastructure.tfstate

aws s3api get-object \
  --bucket axionvera-network-terraform-state \
  --key network-infrastructure.tfstate \
  --version-id <VERSION_ID> \
  terraform.tfstate.backup
```
