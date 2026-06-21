import fs from 'node:fs';
import path from 'node:path';

const root = process.cwd();
const contractsDir = path.join(root, 'shared', 'contracts');

const requiredJson = [
  'models-v1.schema.json',
  'sync-v1.schema.json',
  'error-codes.json',
];

for (const file of requiredJson) {
  JSON.parse(fs.readFileSync(path.join(contractsDir, file), 'utf8'));
}

const openapi = fs.readFileSync(path.join(contractsDir, 'openapi.yaml'), 'utf8');
const syncSchema = JSON.parse(fs.readFileSync(path.join(contractsDir, 'sync-v1.schema.json'), 'utf8'));
const modelsSchema = JSON.parse(fs.readFileSync(path.join(contractsDir, 'models-v1.schema.json'), 'utf8'));

const requiredTables = ['tasks', 'task_columns', 'task_groups', 'notes', 'schedules'];
const requiredOperations = ['insert', 'update', 'delete'];
const syncTables = syncSchema.$defs.SyncTableName.enum;
const syncOperations = syncSchema.$defs.SyncOperation.enum;

for (const table of requiredTables) {
  if (!syncTables.includes(table) || !openapi.includes(table)) {
    throw new Error(`Missing sync table in contracts: ${table}`);
  }
}

for (const operation of requiredOperations) {
  if (!syncOperations.includes(operation) || !openapi.includes(operation)) {
    throw new Error(`Missing sync operation in contracts: ${operation}`);
  }
}

for (const model of ['Task', 'TaskColumn', 'TaskGroup', 'Note', 'Schedule']) {
  if (!modelsSchema.$defs[model]) {
    throw new Error(`Missing model schema: ${model}`);
  }
}

for (const pathName of ['/auth/pair', '/sync/push', '/sync/pull']) {
  if (!openapi.includes(pathName)) {
    throw new Error(`Missing OpenAPI path: ${pathName}`);
  }
}

console.log('contracts ok');
