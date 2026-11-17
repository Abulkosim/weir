import SpeedTest from '@cloudflare/speedtest';
import { PINNED_MEASUREMENTS } from './measurements.ts';

declare global {
  interface Window {
    __twin?: Record<string, number | null>;
    __twinErrors: string[];
    __twinStart?: () => void;
  }
}

window.__twinErrors = [];
const state = document.getElementById('state');

const num = (value: number | null | undefined) => (typeof value === 'number' && Number.isFinite(value) ? value : null);
const mbps = (bps: number | undefined) => (typeof bps === 'number' ? bps / 1e6 : null);

window.__twinStart = () => {
  const began = performance.now();
  const test = new SpeedTest({
    autoStart: false,
    downloadApiUrl: `${location.origin}/__down`,
    uploadApiUrl: `${location.origin}/__up`,
    logAimApiUrl: null,
    logMeasurementApiUrl: null,
    turnServerUri: null,
    turnServerCredsApiUrl: null,
    measurements: PINNED_MEASUREMENTS,
    abortTestDuration: 150_000,
  } as ConstructorParameters<typeof SpeedTest>[0]);
  test.onError = (message: string) => window.__twinErrors.push(String(message).slice(0, 200));
  test.onFinish = (results) => {
    window.__twin = {
      down: mbps(results.getDownloadBandwidth()),
      up: mbps(results.getUploadBandwidth()),
      idle: num(results.getUnloadedLatency()),
      jitter: num(results.getUnloadedJitter()),
      downLoaded: num(results.getDownLoadedLatency()),
      upLoaded: num(results.getUpLoadedLatency()),
      durationMs: performance.now() - began,
    };
    if (state) state.textContent = 'done';
  };
  if (state) state.textContent = 'running';
  test.play();
};

if (state) state.textContent = 'ready';
