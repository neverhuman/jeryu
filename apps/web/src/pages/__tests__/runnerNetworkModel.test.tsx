// runnerNetworkModel.test.tsx — pure runner-network selector coverage.

import { describe, expect, it } from 'vitest';

import {
  lastTtyLine,
  runnerNetworkFromResponse,
} from '../runnerNetworkModel';
import type { RunnerFabricResponse } from '../../api/types';

const EMPTY_RUNNERS: RunnerFabricResponse = {
  schemaVersion: 'jeryu.runner_fabric/v1',
  local: {
    state: 'unknown',
    nodes: 0,
    onlineRunners: 0,
    offlineRunners: 0,
    busyRunners: 0,
    idleRunners: 0,
    totalSlots: 0,
    activeSlots: 0,
    utilization: 0,
    lastUpdated: null,
    nodeDetails: [],
  },
  mirror: {
    name: 'github_actions_runners',
    state: 'missing',
    reason: 'optional GitHub mirror runner adapter is not configured',
    docsUrl: 'docs/agent-native-standard.md',
  },
};

describe('runnerNetworkModel', () => {
  it('returns an empty network when the runners payload is empty', () => {
    const state = runnerNetworkFromResponse(EMPTY_RUNNERS);
    expect(state.state).toBe('unknown');
    expect(state.nodes).toEqual([]);
    expect(state.totals.nodes).toBe(0);
    expect(state.totals.activeTasks).toBe(0);
  });

  it('normalizes offline nodes without inventing activity', () => {
    const state = runnerNetworkFromResponse({
      ...EMPTY_RUNNERS,
      local: {
        ...EMPTY_RUNNERS.local,
        state: 'fresh',
        nodes: 1,
        onlineRunners: 0,
        offlineRunners: 1,
        totalSlots: 10,
        activeSlots: 0,
        nodeDetails: [
          {
            runnerId: 'xbabe1',
            source: 'runnerd',
            state: 'dead',
            capacity: 10,
            inFlight: 0,
            labels: ['dogfood'],
            classes: ['native-rust-hot'],
            activeTaskCount: 0,
            lastUpdated: '2026-06-05T00:00:00Z',
            activeTasks: [],
          },
        ],
      },
    });
    expect(state.nodes[0].runnerId).toBe('xbabe1');
    expect(state.nodes[0].availability).toBe('offline');
    expect(state.nodes[0].activityState).toBe('unknown');
    expect(state.nodes[0].activeTaskCount).toBe(0);
  });

  it('keeps active nodes and task previews intact', () => {
    const state = runnerNetworkFromResponse({
      ...EMPTY_RUNNERS,
      local: {
        state: 'fresh',
        nodes: 2,
        onlineRunners: 2,
        offlineRunners: 0,
        busyRunners: 1,
        idleRunners: 1,
        totalSlots: 20,
        activeSlots: 20,
        utilization: 0.05,
        lastUpdated: '2026-06-05T00:05:00Z',
        nodeDetails: [
          {
            runnerId: 'xbabe0',
            source: 'runnerd',
            state: 'active',
            capacity: 10,
            inFlight: 1,
            labels: ['rust', 'dogfood'],
            classes: ['native-rust-clean'],
            activeTaskCount: 1,
            lastUpdated: '2026-06-05T00:05:00Z',
            activeTasks: [
              {
                taskId: 'ar-1',
                jobId: 'wc-1',
                agentRunId: 'ar-1',
                workcellId: 'wc-1',
                repo: 'jeryu/veox',
                label: 'editbot',
                program: '/workspace/repair.sh',
                state: 'running',
                startedAt: '2026-06-05T00:00:00Z',
                updatedAt: '2026-06-05T00:05:00Z',
                ttyPreview: {
                  state: 'fresh',
                  lines: ['$ repair.sh', 'running tests', 'done'],
                },
              },
            ],
          },
          {
            runnerId: 'local',
            source: 'local',
            state: 'active',
            capacity: 2,
            inFlight: 1,
            labels: ['local'],
            classes: ['native-rust-hot'],
            activeTaskCount: 1,
            lastUpdated: '2026-06-05T00:03:00Z',
            activeTasks: [
              {
                taskId: 'ar-local',
                jobId: 'wc-local',
                agentRunId: 'ar-local',
                workcellId: 'wc-local',
                repo: null,
                label: 'local-repair',
                program: '/workspace/local.sh',
                state: 'running',
                startedAt: '2026-06-05T00:01:00Z',
                updatedAt: '2026-06-05T00:03:00Z',
                ttyPreview: {
                  state: 'missing',
                  lines: [],
                },
              },
            ],
          },
        ],
      },
    });

    expect(state.nodes.map((node) => node.runnerId)).toEqual(['local', 'xbabe0']);
    expect(state.nodes[1].activityState).toBe('active');
    expect(state.nodes[1].tasks[0].lastTtyLine).toBe('done');
    expect(state.nodes[0].tasks[0].lastTtyLine).toBeNull();
    expect(state.totals.activeTasks).toBe(2);
    expect(state.lastUpdated).toBe('2026-06-05T00:05:00Z');
  });

  it('extracts the final non-empty tty line from preview chunks', () => {
    expect(
      lastTtyLine({
        state: 'fresh',
        lines: ['first line', '  ', 'second line\nthird line'],
      })
    ).toBe('third line');
    expect(lastTtyLine({ state: 'missing', lines: [] })).toBeNull();
  });
});
