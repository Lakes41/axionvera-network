# Memory Leak Profiling & Event Loop Optimization Guide

**Issue:** #62  
**Goal:** Achieve 500+ TPS with zero memory leaks  
**Status:** Memory Profiling & Optimization Implementation  
**Last Updated:** 2026-03-30

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Memory Profiling Guide](#memory-profiling-guide)
3. [Event Loop Optimization](#event-loop-optimization)
4. [Stream Processing Implementation](#stream-processing-implementation)
5. [Docker Memory Configuration](#docker-memory-configuration)
6. [Performance Testing](#performance-testing)
7. [Monitoring & Alerting](#monitoring--alerting)

---

## Executive Summary

This guide addresses memory leak issues during high-volume transaction processing (500+ TPS). The solution involves:

1. **Deep-dive profiling** using clinic.js to identify memory-intensive functions
2. **Event loop optimization** - convert synchronous loops to async batches
3. **Stream processing** for large data ingestion (not buffer-loading)
4. **Docker memory limits** to prevent OOM kills with proper configuration

### Expected Outcomes
- ✅ Identify and eliminate memory leaks
- ✅ Reduce event loop blocking
- ✅ Support 500+ TPS consistently
- ✅ 0 OOM errors under peak load
- ✅ Memory growth < 100MB over 24 hours

---

## Memory Profiling Guide

### 1. Using clinic.js (Recommended)

#### Installation

```bash
npm install --save-dev clinic
npm install --save-dev autocannon  # For load testing
```

#### Profiling Commands

**Heap Profiling:**
```bash
# Profile the entire application
clinic doctor -- node src/index.js

# Profile with custom environment
NODE_ENV=production clinic doctor -- node src/index.js
```

**Detailed Analysis:**
```bash
# Allocations profiler (shows where objects are created)
clinic allocations -- node src/index.js

# Flame graph of CPU and memory
clinic flame -- node src/index.js
```

#### Interpreting Results

**clinic.js output directory structure:**
```
.clinic/
├── doctor-*.data          # Main profiling data
├── allocations-*.data     # Allocation patterns
├── flame-*.data          # Flame graphs
└── doctor-*.html         # HTML report
```

**Key indicators to examine:**
- **Heap growth rate** - Should stabilize after initial load
- **GC pauses** - Should be < 100ms at 500+ TPS
- **Event loop delay** - Should be < 10ms average
- **Active handles** - Should not grow unbounded

---

### 2. Using node --inspect

#### Enable Inspector

```bash
# Start with native inspector
node --inspect --expose-gc src/index.js

# With custom host/port
node --inspect=0.0.0.0:9229 src/index.js
```

#### DevTools Integration

```bash
# Chrome DevTools: chrome://inspect in Chrome browser
# Automatically detects running Node.js processes

# VS Code debugger can also attach
```

**Memory profiling steps in DevTools:**
1. Open DevTools → Memory tab
2. Take heap snapshot (baseline)
3. Run load test for 60 seconds
4. Take another heap snapshot
5. Compare snapshots to find retained objects

#### Command Line Profiling

```bash
# Using node-inspect-pro
npm install -g node-inspect-pro
node-inspect-pro src/index.js --duration=60
```

---

### 3. Key Functions to Profile

Focus profiling efforts on:

**Transaction processing:**
```javascript
// High-volume, repeated calls
processTransaction(tx) { ... }
validateTransaction(tx) { ... }
storeTransaction(tx) { ... }
```

**Database operations:**
```javascript
// Potential buffer accumulation
queryLargeDataset() { ... }
batchInsert(rows) { ... }
```

**Event handlers:**
```javascript
// Event accumulation on listeners
on('transaction', handler)
on('block', handler)
```

**Cache operations:**
```javascript
// Unbounded cache growth
cacheTransaction(tx) { ... }
cacheLookup(key) { ... }
```

---

## Event Loop Optimization

### 1. Identifying Blocking Code

**Red flags:**
```javascript
// ❌ Synchronous, blocks event loop
for (let i = 0; i < 100000; i++) {
  heavyComputation(i);
}

// ❌ Large synchronous data processing
const allData = fs.readFileSync(largeFile);
processEntireBuffer(allData);

// ❌ Synchronous database queries (if applicable)
const result = db.querySync("SELECT * FROM transactions");
```

#### Detection Tools

**autocannon load test:**
```bash
# Create load while monitoring event loop delay
autocannon -c 100 -d 60 -p 10 http://localhost:3000
```

**clinic.js latency tracking:**
```bash
clinic doctor --quiet -- node src/index.js
```

### 2. Converting Synchronous to Asynchronous

**Pattern 1: Batch Processing**

```javascript
// ❌ Before: Blocks event loop for large batches
function processBatch(transactions) {
  transactions.forEach(tx => {
    validateAndStore(tx);
  });
}

// ✅ After: Async batching with setImmediate
async function processBatch(transactions) {
  for (let i = 0; i < transactions.length; i += BATCH_SIZE) {
    const batch = transactions.slice(i, i + BATCH_SIZE);
    
    await Promise.all(
      batch.map(tx => validateAndStoreAsync(tx))
    );
    
    // Allow other tasks between batches
    await new Promise(resolve => setImmediate(resolve));
  }
}

const BATCH_SIZE = 100;  // Tune based on profiling
```

**Pattern 2: Work Queues**

```javascript
// ✅ Using a queue to process work asynchronously
const pQueue = require('p-queue');

const executor = new pQueue({
  concurrency: 50,        // Parallel workers
  interval: 1000,         // Per interval...
  intervalCap: 500,       // ...max 500 tasks
  timeout: 30000,         // 30s timeout per task
  throwOnTimeout: true
});

async function handleTransaction(tx) {
  return executor.add(() => processTransaction(tx));
}
```

**Pattern 3: Worker Threads**

```javascript
// ✅ For CPU-intensive work, use worker threads
const { Worker } = require('worker_threads');
const path = require('path');

function createComputeWorker() {
  return new Worker(path.join(__dirname, 'worker.js'));
}

async function heavyCompute(data) {
  return new Promise((resolve, reject) => {
    const worker = createComputeWorker();
    
    worker.on('message', resolve);
    worker.on('error', reject);
    worker.on('exit', (code) => {
      if (code !== 0) reject(new Error(`Worker stopped with code ${code}`));
    });
    
    worker.postMessage(data);
  });
}
```

### 3. Event Loop Monitoring

**Add monitoring to track delays:**

```javascript
// Monitor event loop every 100ms
const lag = require('event-loop-lag');

setInterval(() => {
  const delay = lag();
  
  if (delay > 100) {
    console.warn(`⚠️  Event loop lag detected: ${delay}ms`);
    metrics.recordEventLoopLag(delay);
  }
}, 100);

// Recommended thresholds
const THRESHOLDS = {
  warning: 50,    // >50ms = slow
  critical: 100,  // >100ms = critical
};
```

---

## Stream Processing Implementation

### 1. Large Data Ingestion with Streams

**Problem: Buffer-based loading**

```javascript
// ❌ Loads entire file into memory - causes leaks at scale
async function ingestData(filePath) {
  const data = fs.readFileSync(filePath);  // Could be GBs
  const parsed = JSON.parse(data);
  return processAllAtOnce(parsed);
}
```

**Solution: Stream-based processing**

```javascript
// ✅ Streams only small chunks in memory
async function ingestDataStream(filePath) {
  return new Promise((resolve, reject) => {
    let lineCount = 0;
    
    fs.createReadStream(filePath, { 
      encoding: 'utf8',
      highWaterMark: 64 * 1024  // 64KB chunks
    })
    .pipe(readline.createInterface({
      input: process.stdin,
      crlfDelay: Infinity
    }))
    .on('line', async (line) => {
      try {
        const record = JSON.parse(line);
        await processRecord(record);
        lineCount++;
        
        // Log progress
        if (lineCount % 10000 === 0) {
          console.log(`Processed ${lineCount} records`);
        }
      } catch (err) {
        reject(err);
      }
    })
    .on('error', reject)
    .on('close', () => resolve(lineCount));
  });
}
```

### 2. Transform Streams for Processing

```javascript
const { Transform } = require('stream');

// Custom transform stream for validation
class TransactionValidator extends Transform {
  constructor(options) {
    super({ objectMode: true, ...options });
  }
  
  _transform(transaction, encoding, callback) {
    try {
      const isValid = this.validateTransaction(transaction);
      
      if (isValid) {
        this.push(transaction);  // Valid - pass through
      } else {
        this.emit('invalid', transaction);  // Invalid - emit error event
      }
      
      callback();
    } catch (err) {
      callback(err);
    }
  }
  
  validateTransaction(tx) {
    // Validation logic
    return tx.id && tx.amount > 0;
  }
}

// Usage
fs.createReadStream('transactions.jsonl')
  .pipe(JSONStream.parse('*'))
  .pipe(new TransactionValidator())
  .pipe(storeStream)
  .on('error', (err) => console.error('Pipeline error:', err));
```

### 3. Backpressure Handling

```javascript
// ✅ Handle backpressure to prevent memory buildup
async function pipeWithBackpressure(source, destination) {
  return new Promise((resolve, reject) => {
    source.on('data', (chunk) => {
      const canContinue = destination.write(chunk);
      
      if (!canContinue) {
        // Destination buffer full - pause source
        source.pause();
      }
    });
    
    destination.on('drain', () => {
      // Destination drained - resume source
      source.resume();
    });
    
    source.on('end', () => {
      destination.end();
      resolve();
    });
    
    source.on('error', reject);
    destination.on('error', reject);
  });
}
```

---

## Docker Memory Configuration

### 1. Dockerfile Optimization

```dockerfile
FROM node:18-alpine

WORKDIR /app

# Copy minimal files
COPY package*.json ./
COPY src/ ./src/

# Install production dependencies only
RUN npm ci --only=production

# Expose metrics port
EXPOSE 3000 9090

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD node healthcheck.js

# Set memory limits and garbage collection
ENV NODE_OPTIONS="--max-old-space-size=2048 --expose-gc"

# Run with unprivileged user
USER node

CMD ["node", "src/index.js"]
```

### 2. Docker Compose with Memory Limits

```yaml
version: '3.9'

services:
  axionvera-node:
    build: .
    environment:
      NODE_ENV: production
      NODE_OPTIONS: "--max-old-space-size=2048 --expose-gc"
    
    # Memory constraints
    deploy:
      resources:
        limits:
          memory: 3000M      # Hard limit
          cpus: '2'
        reservations:
          memory: 2000M      # Soft limit
          cpus: '1.5'
    
    # Health check
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/health"]
      interval: 30s
      timeout: 5s
      retries: 3
      start_period: 30s
    
    # Logging
    logging:
      driver: "json-file"
      options:
        max-size: "100m"
        max-file: "10"
    
    ports:
      - "3000:3000"
      - "9090:9090"  # Metrics
    
    volumes:
      - ./logs:/app/logs
      - ./data:/app/data
```

### 3. Kubernetes Memory Management

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: axionvera-node
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: axionvera-node
        image: axionvera:latest
        
        # Memory requests and limits
        resources:
          requests:
            memory: "2Gi"
            cpu: "1.5"
          limits:
            memory: "3Gi"
            cpu: "2"
        
        # Liveness probe
        livenessProbe:
          httpGet:
            path: /health
            port: 3000
          initialDelaySeconds: 30
          periodSeconds: 10
          timeoutSeconds: 5
          failureThreshold: 3
        
        # Readiness probe
        readinessProbe:
          httpGet:
            path: /ready
            port: 3000
          initialDelaySeconds: 10
          periodSeconds: 5
          timeoutSeconds: 3
          failureThreshold: 2
        
        env:
        - name: NODE_OPTIONS
          value: "--max-old-space-size=2048 --expose-gc"
        - name: NODE_ENV
          value: production
```

---

## Performance Testing

### 1. Load Testing with autocannon

```bash
# Basic load test
autocannon -c 100 -d 60 -p 10 http://localhost:3000/api/transaction

# With custom request body
autocannon -c 100 -d 60 -p 10 \
  -b '{"amount":100,"recipient":"addr123"}' \
  http://localhost:3000/api/transaction

# Monitor while testing
while true; do
  node -e "console.log(process.memoryUsage())"
  sleep 5
done
```

### 2. Sustained Load Test Script

```javascript
// sustained-load-test.js
const autocannon = require('autocannon');

async function runTest() {
  console.log('Starting 1-hour sustained load test...');
  
  const result = await autocannon({
    url: 'http://localhost:3000',
    connections: 100,
    duration: 3600,             // 1 hour
    pipelining: 10,
    requests: [
      {
        path: '/api/transaction',
        method: 'POST',
        headers: {
          'Content-Type': 'application/json'
        },
        body: JSON.stringify({
          amount: Math.random() * 10000,
          recipient: 'addr' + Math.random().toString(36)
        })
      }
    ]
  });
  
  console.log('Test Results:', result);
  
  // Check for issues
  if (result.errors > 0) {
    console.error(`❌ ${result.errors} errors during test`);
  }
  if (result.timeouts > 0) {
    console.error(`❌ ${result.timeouts} timeouts during test`);
  }
  
  console.log(`✅ Completed: ${result.requests.average} req/s average`);
}

runTest().catch(console.error);
```

### 3. Memory Leak Detection Test

```javascript
// memory-leak-test.js
const request = require('supertest');

async function testForMemoryLeak() {
  const iterations = 10000;
  const baselineMemory = process.memoryUsage().heapUsed;
  
  console.log(`Baseline heap: ${baselineMemory / 1024 / 1024} MB`);
  
  // Run requests
  for (let i = 0; i < iterations; i++) {
    await request('http://localhost:3000')
      .post('/api/transaction')
      .send({ amount: 100 });
    
    if ((i + 1) % 1000 === 0) {
      const current = process.memoryUsage().heapUsed;
      const growth = (current - baselineMemory) / 1024 / 1024;
      const perRequest = growth / (i + 1);
      
      console.log(`After ${i + 1} requests: +${growth.toFixed(2)}MB (${perRequest.toFixed(4)}MB per req)`);
    }
  }
  
  const finalMemory = process.memoryUsage().heapUsed;
  const totalGrowth = (finalMemory - baselineMemory) / 1024 / 1024;
  
  console.log(`\nTotal growth: ${totalGrowth.toFixed(2)}MB`);
  console.log(totalGrowth < 50 ? '✅ No memory leak detected' : '❌ Potential memory leak');
}

testForMemoryLeak().catch(console.error);
```

---

## Monitoring & Alerting

### 1. Prometheus Metrics

```javascript
// metrics.js
const prometheus = require('prom-client');

// Memory metrics
const heapUsed = new prometheus.Gauge({
  name: 'nodejs_heap_used_bytes',
  help: 'Memory used in bytes',
});

const heapSize = new prometheus.Gauge({
  name: 'nodejs_heap_size_bytes',
  help: 'Total heap size in bytes',
});

const eventLoopLag = new prometheus.Histogram({
  name: 'nodejs_event_loop_lag_ms',
  help: 'Event loop lag in milliseconds',
  buckets: [1, 5, 10, 50, 100, 500],
});

const transactionRate = new prometheus.Counter({
  name: 'transactions_processed_total',
  help: 'Total transactions processed',
});

// Update metrics every 5 seconds
setInterval(() => {
  const mem = process.memoryUsage();
  heapUsed.set(mem.heapUsed);
  heapSize.set(mem.heapTotal);
}, 5000);

module.exports = {
  heapUsed,
  heapSize,
  eventLoopLag,
  transactionRate,
  register: prometheus.register
};
```

### 2. Health Check Endpoint

```javascript
// health.js
const { transactionRate } = require('./metrics');

app.get('/health', (req, res) => {
  const mem = process.memoryUsage();
  const heapPercent = (mem.heapUsed / mem.heapTotal) * 100;
  
  const health = {
    status: heapPercent < 90 ? 'healthy' : 'warning',
    timestamp: new Date().toISOString(),
    memory: {
      heapUsed: `${(mem.heapUsed / 1024 / 1024).toFixed(2)}MB`,
      heapTotal: `${(mem.heapTotal / 1024 / 1024).toFixed(2)}MB`,
      heapPercent: heapPercent.toFixed(2) + '%'
    },
    uptime: process.uptime()
  };
  
  res.status(health.status === 'healthy' ? 200 : 503).json(health);
});

app.get('/metrics', (req, res) => {
  res.set('Content-Type', 'text/plain');
  res.end(require('prom-client').register.metrics());
});
```

---

## Implementation Checklist

### Phase 1: Diagnostics ✅
- [ ] Install clinic.js and autocannon
- [ ] Profile baseline memory usage
- [ ] Identify memory-intensive functions
- [ ] Measure event loop latency

### Phase 2: Optimization ✅
- [ ] Convert blocking loops to async batches
- [ ] Implement stream processing for large data
- [ ] Add backpressure handling
- [ ] Optimize garbage collection

### Phase 3: Configuration ✅
- [ ] Set max-old-space-size in Docker
- [ ] Configure Kubernetes memory requests/limits
- [ ] Add health checks
- [ ] Set up monitoring

### Phase 4: Testing ✅
- [ ] 1-hour sustained load test at 500+ TPS
- [ ] Memory leak detection test
- [ ] Event loop lag monitoring
- [ ] Stress test with OOM scenarios

---

## Performance Targets

| Target | Metric | Goal |
|--------|--------|------|
| **Throughput** | TPS | 500+ (sustained) |
| **Memory** | Heap growth (24h) | < 100MB |
| **Event Loop** | Avg latency | < 10ms |
| **GC Pauses** | Max pause | < 100ms |
| **Errors** | During load test | 0 |
| **Uptime** | MTBF | > 30 days |

---

## References

- [clinic.js documentation](https://clinicjs.org/)
- [Node.js Performance Best Practices](https://nodejs.org/en/docs/guides/nodejs-performance/)
- [Node.js Streams Handbook](https://github.com/substack/stream-handbook)
- [Kubernetes Memory Management](https://kubernetes.io/docs/tasks/manage-memory/)
