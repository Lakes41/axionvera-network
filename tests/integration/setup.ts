import { PostgreSqlContainer } from '@testcontainers/postgresql';
import { GenericContainer, Wait, Network } from 'testcontainers';
import { execSync } from 'child_process';

let postgresContainer: any;
let networkNodeContainer: any;
let network: any;

export async function setup() {
  console.log('🚀 Starting integration test environment...');
  
  try {
    network = await new Network().start();

    // Start PostgreSQL container
    console.log('📦 Starting PostgreSQL container...');
    postgresContainer = await new PostgreSqlContainer('postgres:15-alpine')
      .withDatabase('testdb')
      .withUsername('testuser')
      .withPassword('testpass')
      .withExposedPorts(5432)
      .withNetwork(network)
      .withNetworkAliases('postgres')
      .start();
    
    const dbHost = postgresContainer.getHost();
    const dbPort = postgresContainer.getMappedPort(5432);
    const dbUrl = `postgresql://testuser:testpass@${dbHost}:${dbPort}/testdb`;
    
    console.log(`✅ PostgreSQL started on ${dbHost}:${dbPort}`);
    
    // Set environment variables for tests
    process.env.TEST_DATABASE_URL = dbUrl;
    process.env.TEST_POSTGRES_HOST = dbHost;
    process.env.TEST_POSTGRES_PORT = dbPort.toString();
    
    // Start network-node container
    console.log('📦 Starting network-node container...');
    const nodeBuilder = await GenericContainer.fromDockerfile('./network-node');
    networkNodeContainer = await nodeBuilder
      .build()
      .then((c) =>
        c
          .withExposedPorts(50051, 9090)
          .withNetwork(network)
          .withEnvironment({
            DATABASE_URL: 'postgresql://testuser:testpass@postgres:5432/testdb',
            RUST_LOG: 'info',
          })
          .withWaitStrategy(Wait.forLogMessage(/gRPC server listening on/))
          .start()
      );
    
    const nodeHost = networkNodeContainer.getHost();
    const nodePort = networkNodeContainer.getMappedPort(50051);
    const metricsPort = networkNodeContainer.getMappedPort(9090);
    
    process.env.TEST_NODE_HOST = nodeHost;
    process.env.TEST_NODE_PORT = nodePort.toString();
    process.env.TEST_METRICS_PORT = metricsPort.toString();
    
    console.log(`✅ Network node started on ${nodeHost}:${nodePort}`);
    console.log('✅ Integration test environment ready');
  } catch (error) {
    console.error('❌ Failed to setup integration test environment:', error);
    throw error;
  }
}

export async function teardown() {
  console.log('🧹 Cleaning up integration test environment...');
  
  try {
    // Stop PostgreSQL container
    if (postgresContainer) {
      console.log('⏹️  Stopping PostgreSQL container...');
      await postgresContainer.stop({ timeout: 10000 });
      console.log('✅ PostgreSQL container stopped');
    }
    
    // Stop network-node container if running
    if (networkNodeContainer) {
      console.log('⏹️  Stopping network-node container...');
      await networkNodeContainer.stop({ timeout: 10000 });
      console.log('✅ Network-node container stopped');
    }

    if (network) {
      console.log('⏹️  Stopping test network...');
      await network.stop();
      console.log('✅ Test network stopped');
    }
    
    // Clean up any dangling containers and volumes
    console.log('🧹 Cleaning up Docker resources...');
    try {
      execSync('docker container prune -f', { stdio: 'ignore' });
      execSync('docker volume prune -f', { stdio: 'ignore' });
      console.log('✅ Docker cleanup complete');
    } catch (cleanupError) {
      console.warn('⚠️  Docker cleanup failed (non-critical):', cleanupError);
    }
    
    console.log('✅ Integration test environment cleaned up');
  } catch (error) {
    console.error('❌ Error during teardown:', error);
    // Don't throw in teardown to avoid masking test failures
  }
}

export default async function () {
  await setup();
  return async () => {
    await teardown();
  };
}
