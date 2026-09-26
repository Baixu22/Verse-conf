#!/usr/bin/env node
// TF-0073 的回归：分析口径必须真的按「来源」这个独立单元来算。
//
//   node --test benchmark/analysis/
//
// 最重要的一条是 `concentrated_evidence_gets_a_much_wider_interval_than_spread_evidence`：
// 同样的通过率、同样的运行次数，只把失败集中到一个来源里，区间就必须显著变宽。
// 这一条如果不成立，整个 TF-0073 就没有意义——报告会把「一份文件的特性」
// 说成普遍规律，而区间看起来还很窄。

import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  DEFAULT_SEED,
  buildReport,
  clusterBootstrap,
  countUnits,
  parseRuns,
  renderMarkdown,
  weightedSummary,
} from './cluster_report.mjs';

const ENDING_OF = (index) => (index % 2 === 0 ? 'LF' : 'CRLF');

/** 造一批运行：每个来源 1 份文档、4 个任务、每个任务 3 次重复 */
function makeRuns(clusterCount, { fail = () => false, tasksPerCluster = 4, repeats = 3 } = {}) {
  const runs = [];
  for (let c = 0; c < clusterCount; c += 1) {
    const cluster = `c${String(c).padStart(2, '0')}`;
    for (let t = 0; t < tasksPerCluster; t += 1) {
      for (let r = 0; r < repeats; r += 1) {
        const ok = !fail(cluster, t, r);
        runs.push({
          cluster,
          document: `${cluster}.toml`,
          ending: ENDING_OF(c),
          arm: 'intent',
          model: 'm1',
          task: `${cluster}-task-${t}`,
          repeat: r,
          semantic_ok: ok,
          byte_ok: ok,
          applied: true,
        });
      }
    }
  }
  return runs;
}

function width(estimate) {
  return estimate.high - estimate.low;
}

test('every_nesting_level_is_counted_separately', () => {
  const runs = makeRuns(16);
  assert.deepEqual(countUnits(runs), { runs: 192, tasks: 64, documents: 16, clusters: 16 });

  const report = buildReport(runs);
  assert.equal(report.units.runs, 192);
  assert.equal(report.units.clusters, 16);
  assert.match(report.independence_note, /独立单元是\*\*来源\*\*/);
  assert.match(report.independence_note, /192 次运行来自 64 个任务、16 份文档、16 个来源/);
});

test('concentrated_evidence_gets_a_much_wider_interval_than_spread_evidence', () => {
  // 同样的通过率、同样的运行次数，唯一的差别是失败落在哪里。
  // 这就是 TF-0073 存在的原因：不按来源聚类就看不见这件事。
  const concentrated = makeRuns(16, {
    fail: (cluster) => cluster === 'c00',
  });
  const spread = makeRuns(16, {
    fail: (cluster, task, repeat) => task === 0 && repeat === 0 && Number(cluster.slice(1)) < 12,
  });

  const concentratedEstimate = clusterBootstrap(concentrated, (run) => run.byte_ok);
  const spreadEstimate = clusterBootstrap(spread, (run) => run.byte_ok);

  assert.equal(concentratedEstimate.point.rate, spreadEstimate.point.rate, '两者的点估计必须相同');

  const concentratedWidth = width(concentratedEstimate);
  const spreadWidth = width(spreadEstimate);
  assert.ok(
    concentratedWidth > spreadWidth * 2,
    `失败集中在一个来源时区间必须显著更宽：集中 ${concentratedWidth.toFixed(4)} vs 分散 ${spreadWidth.toFixed(4)}`,
  );
});

test('the_bootstrap_is_deterministic_for_a_fixed_seed', () => {
  const runs = makeRuns(24, { fail: (cluster, task) => cluster === 'c01' && task % 2 === 0 });
  const first = clusterBootstrap(runs, (run) => run.byte_ok, { seed: 7, iterations: 500 });
  const second = clusterBootstrap(runs, (run) => run.byte_ok, { seed: 7, iterations: 500 });
  assert.deepEqual(first, second, '同一种子必须得到逐字段相同的结果');
  assert.ok(first.low < first.high, '区间不能退化成一点');

  const other = clusterBootstrap(runs, (run) => run.byte_ok, { seed: 8, iterations: 500 });
  assert.notDeepEqual(first, other, '换种子应当得到不同的重抽结果');
  assert.equal(DEFAULT_SEED, 20260926);
});

test('a_single_cluster_cannot_produce_an_interval', () => {
  const runs = makeRuns(1);
  const estimate = clusterBootstrap(runs, (run) => run.byte_ok);
  assert.equal(estimate.usable, false);
  assert.match(estimate.reason, /只有 1 个来源/);
});

test('oversampling_crlf_is_never_reported_as_a_production_average', () => {
  // 语料是 LF/CRLF 各半，而预登记的使用分布假设真实配置绝大多数是 LF。
  const runs = makeRuns(16, { fail: (cluster, task) => cluster === 'c00' && task === 0 });
  const weighted = weightedSummary(runs, { LF: 0.9, CRLF: 0.1 }, { weightsSource: 'preregistered' });

  assert.equal(weighted.oversampled, true, '语料分布与目标分布差 0.4，必须判为过采样');
  assert.equal(weighted.is_production_average, false, '加权汇总永远不是生产环境平均值');
  assert.match(weighted.label, /\*\*不是\*\*生产环境平均值/);

  const markdown = renderMarkdown(buildReport(runs, { weights: { LF: 0.9, CRLF: 0.1 } }));
  assert.match(markdown, /过采样警告/);
  assert.match(markdown, /说成生产环境平均值/);
});

test('a_matching_distribution_still_does_not_call_itself_a_production_average', () => {
  const runs = makeRuns(16);
  const weighted = weightedSummary(runs, { LF: 0.5, CRLF: 0.5 });
  assert.equal(weighted.oversampled, false);
  assert.equal(weighted.is_production_average, false, '权重是假设，不是测量结果');
  assert.match(weighted.label, /权重是假设，不是测量结果/);
});

test('weights_must_sum_to_one', () => {
  const runs = makeRuns(4);
  assert.throws(() => weightedSummary(runs, { LF: 0.8, CRLF: 0.8 }), /权重之和必须是 1/);
});

test('the_weighted_summary_is_omitted_when_no_weights_are_preregistered', () => {
  const report = buildReport(makeRuns(4));
  assert.equal(report.weighted, null);
  assert.match(report.weighted_note, /不出加权汇总/);
  assert.match(renderMarkdown(report), /未给预登记使用分布/);
});

test('an_input_missing_a_required_field_is_rejected_loudly', () => {
  const line = JSON.stringify({
    document: 'a.toml',
    ending: 'LF',
    arm: 'intent',
    model: 'm1',
    task: 't',
    repeat: 0,
    semantic_ok: true,
    byte_ok: true,
  });
  assert.throws(() => parseRuns(line, 'x.jsonl'), /缺少字段 'cluster'/);
});

test('an_unknown_line_ending_is_rejected_instead_of_being_bucketed', () => {
  const line = JSON.stringify({
    cluster: 'a',
    document: 'a.toml',
    ending: 'UNKNOWN',
    arm: 'intent',
    model: 'm1',
    task: 't',
    repeat: 0,
    semantic_ok: true,
    byte_ok: true,
  });
  assert.throws(() => parseRuns(line, 'x.jsonl'), /只接受 LF \/ CRLF/);
});

test('silent_miswrites_are_counted_by_source_not_by_run', () => {
  const runs = makeRuns(16, { fail: (cluster) => cluster === 'c03' });
  for (const run of runs) {
    if (run.cluster === 'c03') {
      run.semantic_ok = true;
      run.byte_ok = false;
      run.applied = true;
    }
  }
  const report = buildReport(runs);
  assert.equal(report.silent_miswrites.runs, 12);
  assert.equal(report.silent_miswrites.documents, 1);
  assert.equal(report.silent_miswrites.clusters, 1, '12 次静默误改只来自 1 个来源');
});
