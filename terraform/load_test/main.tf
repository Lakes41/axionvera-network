# terraform/load_test/main.tf

provider "aws" {
  region = var.aws_region
}

resource "aws_ecs_cluster" "load_test" {
  name = "load-test-cluster"
  
  setting {
    name  = "containerInsights"
    value = "enabled"
  }
}

resource "aws_ecs_task_definition" "load_generator" {
  family                   = "k6-load-generator"
  network_mode             = "awsvpc"
  requires_compatibilities = ["FARGATE"]
  cpu                      = 256
  memory                   = 512

  container_definitions = jsonencode([
    {
      name  = "k6"
      image = "grafana/k6:latest"
      command = ["run", "/scripts/load_test.js", "--env", "BASE_URL=${var.target_url}"]
      environment = [
        { name = "BASE_URL", value = var.target_url }
      ]
      logConfiguration = {
        logDriver = "awslogs"
        options = {
          "awslogs-group"         = "/ecs/load-test"
          "awslogs-region"        = var.aws_region
          "awslogs-stream-prefix" = "ecs"
        }
      }
    }
  ])
}

resource "aws_ecs_service" "load_test_service" {
  name            = "load-test-service"
  cluster         = aws_ecs_cluster.load_test.id
  task_definition = aws_ecs_task_definition.load_generator.arn
  desired_count   = var.desired_count
  launch_type     = "FARGATE"

  network_configuration {
    subnets          = var.subnet_ids
    security_groups  = [aws_security_group.load_test_sg.id]
    assign_public_ip = true
  }
}

resource "aws_security_group" "load_test_sg" {
  name        = "load-test-sg"
  vpc_id      = var.vpc_id

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

# terraform/load_test/variables.tf

variable "aws_region" {
  default = "us-west-2"
}

variable "target_url" {
  description = "The target endpoint to test"
}

variable "desired_count" {
  default = 5
}

variable "vpc_id" {}
variable "subnet_ids" {
  type = list(string)
}

# terraform/load_test/outputs.tf

output "cluster_name" {
  value = aws_ecs_cluster.load_test.name
}

output "service_name" {
  value = aws_ecs_service.load_test_service.name
}
