import { useState } from "react";
import { AlertDialog, Button, Flex } from "@radix-ui/themes";

export type DiagnosticDeletion = { kind: "current"; id: string } | { kind: "all" };

interface Props {
  diagnosticId: string;
  disabled: boolean;
  onDelete: (target: DiagnosticDeletion) => Promise<void>;
}

/** Confirmation is UI-only; Rust owns deletion and the resulting record list. */
export function RecognitionDiagnosticDeleteActions({ diagnosticId, disabled, onDelete }: Props) {
  const [pending, setPending] = useState<DiagnosticDeletion | null>(null);
  const all = pending?.kind === "all";
  return (
    <>
      <Flex gap="2" wrap="wrap">
        <Button size="2" variant="ghost" color="red" disabled={disabled} onClick={() => setPending({ kind: "current", id: diagnosticId })}>删除当前记录</Button>
        <Button size="2" variant="ghost" color="gray" disabled={disabled} onClick={() => setPending({ kind: "all" })}>清空全部记录</Button>
      </Flex>
      <AlertDialog.Root open={pending !== null} onOpenChange={(open) => { if (!open) setPending(null); }}>
        <AlertDialog.Content maxWidth="420px">
          <AlertDialog.Title>{all ? "清空全部诊断记录？" : "删除当前诊断记录？"}</AlertDialog.Title>
          <AlertDialog.Description>
            {all ? "将删除本机保存的全部诊断记录、现场截图和问题描述，删除后无法恢复。" : "将删除当前诊断记录、现场截图和问题描述，删除后无法恢复。"}
          </AlertDialog.Description>
          <Flex justify="end" gap="2" mt="4">
            <AlertDialog.Cancel><Button variant="soft" color="gray">取消</Button></AlertDialog.Cancel>
            <AlertDialog.Action>
              <Button color="red" onClick={() => { if (pending) void onDelete(pending); }}>{all ? "确认清空" : "确认删除"}</Button>
            </AlertDialog.Action>
          </Flex>
        </AlertDialog.Content>
      </AlertDialog.Root>
    </>
  );
}
