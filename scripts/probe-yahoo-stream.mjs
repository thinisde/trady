#!/usr/bin/env node
// A ten-second read-only wire diagnostic. No cookies, broker or credentials.
// Wire field numbers match the pinned yfinance-rs 0.9.1 yaticker.proto schema.
import { createHash } from 'node:crypto';
import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

function pricing(message) {
  const bytes = Buffer.from(message, 'base64');
  if (bytes.length > 65536) throw new Error('FRAME_BOUND');
  let offset = 0;
  function integer() {
    let value = 0n;
    for (let index = 0; index < 10 && offset < bytes.length; index++) {
      const byte = bytes[offset++];
      value |= BigInt(byte & 127) << BigInt(index * 7);
      if (!(byte & 128)) return value;
    }
    throw new Error('INVALID_VARINT');
  }
  const result = {};
  const text = { 1: 'symbol', 4: 'currency', 5: 'exchange' };
  const floats = { 2: 'last', 23: 'bid', 25: 'ask' };
  const signed = { 3: 'timestamp_ms', 24: 'bid_size', 26: 'ask_size' };
  while (offset < bytes.length) {
    const tag = Number(integer());
    const field = tag >>> 3;
    const wire = tag & 7;
    if (!field) throw new Error('INVALID_FIELD');
    if (wire === 0) {
      const raw = integer();
      if (signed[field]) result[signed[field]] = Number((raw >> 1n) ^ -(raw & 1n));
    } else if (wire === 2) {
      const length = Number(integer());
      if (length > 65536 || offset + length > bytes.length) throw new Error('INVALID_LENGTH');
      if (text[field]) result[text[field]] = bytes.subarray(offset, offset + length).toString('utf8');
      offset += length;
    } else if (wire === 5 || wire === 1) {
      const length = wire === 5 ? 4 : 8;
      if (offset + length > bytes.length) throw new Error('TRUNCATED_FRAME');
      if (wire === 5 && floats[field]) result[floats[field]] = bytes.readFloatLE(offset);
      offset += length;
    } else throw new Error('UNSUPPORTED_WIRE_TYPE');
  }
  return result;
}

const startedAt = new Date().toISOString();
const frames = [];
let status = 'NO_UPDATES';
let finished = false;
const socket = new WebSocket('wss://streamer.finance.yahoo.com/?version=2');
const timer = setTimeout(finish, 12000);
socket.addEventListener('open', () => {
  socket.send(JSON.stringify({ subscribe: ['AIR.PA', 'ASML.AS'] }));
});
socket.addEventListener('message', event => {
  try {
    if (typeof event.data !== 'string' || event.data.length > 100000) throw new Error('MESSAGE_BOUND');
    const envelope = JSON.parse(event.data);
    if (envelope.type !== 'pricing' || typeof envelope.message !== 'string') return;
    const decoded = pricing(envelope.message);
    if (!['AIR.PA', 'ASML.AS'].includes(decoded.symbol)) throw new Error('UNREQUESTED_SYMBOL');
    frames.push({ received_at: new Date().toISOString(), response_hash: createHash('sha256').update(event.data).digest('hex'),
      wire_response: envelope, decoded });
    status = 'PRICING_FRAMES_RECEIVED';
    if (frames.length >= 32) finish();
  } catch { status = 'DECODE_ERROR'; finish(); }
});
socket.addEventListener('error', () => { status = 'UNAVAILABLE'; finish(); });
socket.addEventListener('close', finish);
function finish() {
  if (finished) return;
  finished = true;
  clearTimeout(timer);
  try { socket.close(); } catch { /* timeout while connecting */ }
  const directory = fileURLToPath(new URL('../var/eur-first-run-checks/', import.meta.url));
  mkdirSync(directory, { recursive: true });
  const path = directory + 'yahoo-stream-' + Date.now() + '.json';
  const summary = frames.map(frame => ({ ...frame.decoded,
    bid_field_present: 'bid' in frame.decoded, ask_field_present: 'ask' in frame.decoded,
    event_age_seconds: frame.decoded.timestamp_ms ? (Date.parse(frame.received_at) - frame.decoded.timestamp_ms) / 1000 : null }));
  writeFileSync(path, JSON.stringify({ schema_version: 1, provider: 'Yahoo Finance raw websocket',
    endpoint: 'wss://streamer.finance.yahoo.com/?version=2', started_at: startedAt,
    received_at: new Date().toISOString(), status, frames, summary, entry_authorized: false, broker_mutations: 0 }, null, 2) + '\n', { mode: 0o600, flag: 'wx' });
  console.log(JSON.stringify({ artifact: path, status, frames: frames.length, summary: summary.slice(-4), entry_authorized: false }));
  process.exit(0);
}
