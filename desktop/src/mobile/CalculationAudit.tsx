import { useState } from "react";

export interface CalculationSummary {
  calculation_id: string;
  record_id?: number;
  node_id: string;
  function: string;
  executed: boolean;
  output: { status: string; value: number | null; unit: string; display: string | null; reason: string };
}
export interface CalculationRecord extends CalculationSummary {
  calc_version: string;
  bindings: unknown;
  inputs: unknown;
  inputs_refs: unknown;
}
export function CalculationAuditView({ record }: { record: CalculationRecord }) {
  return <article className="rw-stage" aria-label="计算输入与依赖审计">
    <h3>{record.function}</h3>
    <p>{record.executed ? "已执行确定性计算" : "节点未执行"} · {record.output.status === "ok" ? record.output.display ?? "多项结果，见明细" : record.output.status === "not_meaningful" ? "该输入下无意义" : "计算未通过"}</p>
    {record.output.reason && <p role="status">{record.output.reason}</p>}
    <small>函数版本 {record.calc_version} · {record.calculation_id}</small>
    <details><summary>计算计划与声明的单位、参数</summary><pre>{JSON.stringify(record.bindings, null, 2)}</pre></details>
    <details><summary>证据和前序计算的来源路径</summary><pre>{JSON.stringify(record.inputs_refs, null, 2)}</pre></details>
    <details><summary>实际参与计算的输入</summary><pre>{JSON.stringify(record.inputs, null, 2)}</pre></details>
    <details><summary>完整输出与中间量</summary><pre>{JSON.stringify(record.output, null, 2)}</pre></details>
  </article>;
}
async function read<T>(taskId: number, calculationId?: string): Promise<T> {
  const params = new URLSearchParams({ task_id: String(taskId) });
  if (calculationId) params.set("calculation_id", calculationId);
  const response = await fetch(`/api/mobile/research/calculations?${params}`);
  const data = await response.json();
  if (!response.ok) throw new Error(data.message ?? "计算审计读取失败");
  return data as T;
}
export function CalculationAuditPanel({ taskId }: { taskId: number }) {
  const [items, setItems] = useState<CalculationSummary[] | null>(null);
  const [record, setRecord] = useState<CalculationRecord | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function open(id?: string) {
    setBusy(true); setError("");
    try {
      if (id) setRecord(await read<CalculationRecord>(taskId, id));
      else setItems((await read<{ records: CalculationSummary[] }>(taskId)).records);
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  }
  return <section className="rw-audit" aria-label="本研究计算审计">
    <button type="button" disabled={busy} onClick={() => void open()}>{busy ? "正在读取审计…" : "查看计算依赖与输入审计"}</button>
    {error && <p role="alert">{error}</p>}
    {items && <><p>共 {items.length} 项不可变记录，包含已执行和被拒绝的尝试；与业务备份一起保存。</p>
      {items.length === 0 && <p>本研究尚无确定性计算记录。</p>}
      {items.map((r) => <button type="button" key={r.calculation_id} disabled={busy} onClick={() => void open(r.calculation_id)}>{r.function} · {r.output.status === "ok" ? r.output.display ?? "多项结果" : r.output.reason || r.output.status}</button>)}
    </>}
    {record && <CalculationAuditView record={record} />}
  </section>;
}
