import { useCallback, useEffect, useState } from "react";
import { Box, Button, Dialog, Flex, Grid, Select, Text, TextArea } from "@radix-ui/themes";
import { convertFileSrc, invoke, listen } from "../../tauri";
import { RecognitionDiagnosticDeleteActions, type DiagnosticDeletion } from "./RecognitionDiagnosticDeleteActions";
import type { RecognitionDiagnosticSummary } from "../../types/recognitionDiagnostics";

/** Rust owns stored records; local state holds UI selection and unsaved input drafts. */
export function RecognitionDiagnosticActions() {
  const [records, setRecords] = useState<RecognitionDiagnosticSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string>();
  const [open, setOpen] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const busy = exporting || deleting;
  const [message, setMessage] = useState("");
  const [descriptionDrafts, setDescriptionDrafts] = useState<Record<string, string>>({});
  const refresh = useCallback(async () => {
    const next = await invoke<RecognitionDiagnosticSummary[]>("list_recognition_diagnostics");
    setRecords(next ?? []);
  }, []);
  useEffect(() => {
    let active = true;
    const load = () => {
      void invoke<RecognitionDiagnosticSummary[]>("list_recognition_diagnostics").then((next) => {
        if (active) setRecords(next ?? []);
      }).catch(() => {});
    };
    load();
    const unlisten = listen("recognition-diagnostic-saved", load);
    const unlistenChanged = listen("recognition-diagnostics-changed", load);
    return () => {
      active = false;
      void unlisten.then((fn) => fn());
      void unlistenChanged.then((fn) => fn());
    };
  }, []);
  const selected = records.find((r) => r.id === selectedId) ?? records[0];
  const description = selected ? descriptionDrafts[selected.id] ?? selected.userDescription ?? "" : "";
  const exportDiagnostic = async () => {
    if (!selected) return;
    setExporting(true);
    setMessage("");
    try {
      const path = await invoke<string | null>("export_recognition_diagnostic", { diagnosticId: selected.id, userDescription: description });
      setMessage(path ? `诊断包已保存：${path}` : "已取消导出");
    } catch (error) { setMessage(`导出失败：${String(error)}`); }
    finally { setExporting(false); }
  };
  const deleteDiagnostics = async (target: DiagnosticDeletion) => {
    setDeleting(true);
    setMessage("");
    try {
      if (target.kind === "all") await invoke("clear_recognition_diagnostics");
      else await invoke("delete_recognition_diagnostic", { diagnosticId: target.id });
      setSelectedId(undefined);
      setDescriptionDrafts((drafts) => {
        if (target.kind === "all") return {};
        const next = { ...drafts };
        delete next[target.id];
        return next;
      });
      await refresh();
      setMessage(target.kind === "all" ? "已清空全部诊断记录" : "诊断记录已删除");
    } catch (error) {
      setMessage(`清除失败：${String(error)}`);
      void refresh().catch(() => {});
    } finally { setDeleting(false); }
  };
  if (!selected && !open) return null;
  return (
    <Dialog.Root open={open} onOpenChange={(value) => { setOpen(value); if (value) void refresh().catch((e) => setMessage(String(e))); }}>
      <Dialog.Trigger><Button size="1" variant="soft">识别失败诊断</Button></Dialog.Trigger>
      <Dialog.Content maxWidth="900px">
        <Dialog.Title>识别失败诊断</Dialog.Title>
        <Dialog.Description size="2">诊断包包含游戏截图和运行日志，设备标识与本机路径会脱敏。请检查画面后分享。</Dialog.Description>
        <Flex direction="column" gap="3" mt="3">
          {selected ? <>
            <Select.Root disabled={busy} value={selected.id} onValueChange={(id) => { setSelectedId(id); setMessage(""); }}>
              <Select.Trigger aria-label="诊断记录" />
              <Select.Content>{records.map((record) => <Select.Item key={record.id} value={record.id}>{new Date(record.createdAtMs).toLocaleString()} · {record.reason}</Select.Item>)}</Select.Content>
            </Select.Root>
            <Text size="2">{selected.reason}</Text>
            {selected.imagePaths.length === 0 && <Text size="2" color="gray">没有可用的现场截图，具体原因见下方缺失证据。</Text>}
            <Grid columns="2" gap="2">
              {selected.imagePaths.map((path, index) => (
                <Box key={path} minWidth="0">
                  <Text as="p" size="1" color="gray" mb="1">现场 {index + 1}</Text>
                  <img className="recognition-diagnostic-thumbnail" src={convertFileSrc(path)} alt={`失败识别现场 ${index + 1}`} />
                </Box>
              ))}
            </Grid>
            <Box>
              <Text as="label" htmlFor="recognition-diagnostic-description" size="2" weight="medium">问题描述（选填）</Text>
              <TextArea
                id="recognition-diagnostic-description"
                mt="2"
                rows={3}
                value={description}
                onChange={(event) => setDescriptionDrafts((drafts) => ({ ...drafts, [selected.id]: event.target.value }))}
                disabled={busy}
                placeholder="可以填写操作步骤、预期结果和实际表现，导出时会一同保存。"
              />
            </Box>
            {selected.missing.length > 0 && <Text size="1" color="gray">部分证据缺失，仍可导出：{selected.missing.join("；")}</Text>}
          </> : <Text size="2" color="gray">暂无诊断记录</Text>}
          {message && <Text size="2" role="status">{message}</Text>}
          <Flex justify="between" align="center" gap="3" wrap="wrap">
            {selected ? <RecognitionDiagnosticDeleteActions diagnosticId={selected.id} disabled={busy} onDelete={deleteDiagnostics} /> : <Box />}
            <Flex gap="2">
              <Dialog.Close><Button variant="soft" color="gray">关闭</Button></Dialog.Close>
              {selected && <Button disabled={busy} onClick={() => void exportDiagnostic()}>{exporting ? "导出中…" : "导出诊断包"}</Button>}
            </Flex>
          </Flex>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}
