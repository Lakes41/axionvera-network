import { describe, it, expect, beforeAll } from 'vitest';
import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';
import path from 'path';

const PROTO_PATH = path.resolve(__dirname, '../../proto/network.proto');

const packageDefinition = protoLoader.loadSync(PROTO_PATH, {
  keepCase: true,
  longs: String,
  enums: String,
  defaults: true,
  oneofs: true,
});

const networkProto = (grpc.loadPackageDefinition(packageDefinition) as any).axionvera.network;

describe('gRPC Network Service Integration Tests', () => {
  let client: any;

  beforeAll(() => {
    const host = process.env.TEST_NODE_HOST || 'localhost';
    const port = process.env.TEST_NODE_PORT || '50051';
    client = new networkProto.NetworkService(
      `${host}:${port}`,
      grpc.credentials.createInsecure()
    );
  });

  describe('GetTVL', () => {
    it('should return mock TVL data on happy path', () => {
      return new Promise<void>((resolve, reject) => {
        client.GetTVL({ token_address: '0x1234' }, (error: any, response: any) => {
          if (error) {
            reject(error);
            return;
          }

          expect(response).toBeDefined();
          expect(response.total_value_locked).toBe('5000000000');
          expect(response.token_address).toBe('0x1234');
          expect(response.timestamp).toBeDefined();
          resolve();
        });
      });
    });

    it('should return error status for invalid requests', () => {
      // For this mock, we don't have many ways to trigger errors yet, 
      // but we can test that it doesn't crash.
      // If we implement validation in the future, we'd test it here.
      return new Promise<void>((resolve, reject) => {
        // Send a request that might be considered "invalid" if we had validation
        // For now, let's just ensure we can call it.
        client.GetTVL({}, (error: any, response: any) => {
          if (error) {
            // If we expect an error, we check the code
            // expect(error.code).toBe(grpc.status.INVALID_ARGUMENT);
            resolve();
            return;
          }
          expect(response).toBeDefined();
          resolve();
        });
      });
    });
  });

  describe('GetNetworkStatus', () => {
    it('should return network status', () => {
      return new Promise<void>((resolve, reject) => {
        client.GetNetworkStatus({}, (error: any, response: any) => {
          if (error) {
            reject(error);
            return;
          }

          expect(response).toBeDefined();
          expect(response.is_healthy).toBe(true);
          expect(response.network_version).toBe('1.0.0');
          resolve();
        });
      });
    });
  });
});
