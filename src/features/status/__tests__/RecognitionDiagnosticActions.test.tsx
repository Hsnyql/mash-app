import { beforeEach, describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { RecognitionDiagnosticActions } from "../RecognitionDiagnosticActions";

const record = { id: "record-1", createdAtMs: 1000, reason: "无法识别当前画面", imagePaths: ["/tmp/frame-1.png", "/tmp/frame-2.png", "/tmp/frame-3.png"], missing: [], userDescription: "" };
beforeEach(() => { vi.mocked(invoke).mockReset(); });

describe("RecognitionDiagnosticActions", () => {
  it("does not show an entry without stored evidence", async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    renderWithTheme(<RecognitionDiagnosticActions />);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("list_recognition_diagnostics"));
    expect(screen.queryByText("识别失败诊断")).not.toBeInTheDocument();
  });
  it("shows the backend frame and exports the selected record", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd) => cmd === "list_recognition_diagnostics" ? [record] : "/tmp/report.zip");
    renderWithTheme(<RecognitionDiagnosticActions />);
    await user.click(await screen.findByRole("button", { name: "识别失败诊断" }));
    expect(screen.getByAltText("失败识别现场 1")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "导出诊断包" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("export_recognition_diagnostic", { diagnosticId: record.id, userDescription: "" }));
    expect(await screen.findByRole("status")).toHaveTextContent("诊断包已保存");
  });
  it("allows incomplete evidence and handles save cancellation", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd) => cmd === "list_recognition_diagnostics" ? [{ ...record, imagePaths: [], missing: ["capture_disabled"] }] : null);
    renderWithTheme(<RecognitionDiagnosticActions />);
    await user.click(await screen.findByRole("button", { name: "识别失败诊断" }));
    expect(screen.getByText(/部分证据缺失/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "导出诊断包" }));
    expect(await screen.findByRole("status")).toHaveTextContent("已取消导出");
  });
  it("reports export errors and allows retry", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === "list_recognition_diagnostics") return [record];
      throw new Error("disk full");
    });
    renderWithTheme(<RecognitionDiagnosticActions />);
    await user.click(await screen.findByRole("button", { name: "识别失败诊断" }));
    await user.click(screen.getByRole("button", { name: "导出诊断包" }));
    expect(await screen.findByRole("status")).toHaveTextContent("导出失败");
    expect(screen.getByRole("button", { name: "导出诊断包" })).toBeEnabled();
  });
  it("includes the description in export and keeps the draft after cancellation", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd) => cmd === "list_recognition_diagnostics" ? [{ ...record, userDescription: "已有描述" }] : null);
    renderWithTheme(<RecognitionDiagnosticActions />);
    await user.click(await screen.findByRole("button", { name: "识别失败诊断" }));
    expect(screen.getAllByRole("img")).toHaveLength(3);
    const input = screen.getByRole("textbox", { name: "问题描述（选填）" });
    expect(input).toHaveValue("已有描述");
    await user.clear(input);
    await user.type(input, "点击开始后卡在助战页面");
    await user.click(screen.getByRole("button", { name: "导出诊断包" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("export_recognition_diagnostic", {
      diagnosticId: record.id, userDescription: "点击开始后卡在助战页面",
    }));
    expect(await screen.findByRole("status")).toHaveTextContent("已取消导出");
    expect(input).toHaveValue("点击开始后卡在助战页面");
  });
  it("keeps descriptions separate when changing records", async () => {
    const user = userEvent.setup();
    const second = { ...record, id: "record-2", reason: "第二次失败", userDescription: "另一条记录的描述" };
    vi.mocked(invoke).mockResolvedValue([record, second]);
    renderWithTheme(<RecognitionDiagnosticActions />);
    await user.click(await screen.findByRole("button", { name: "识别失败诊断" }));
    await user.type(screen.getByRole("textbox"), "第一条的草稿");
    await user.click(screen.getByRole("combobox", { name: "诊断记录" }));
    await user.click(await screen.findByRole("option", { name: /第二次失败/ }));
    expect(screen.getByRole("textbox")).toHaveValue("另一条记录的描述");
    await user.click(screen.getByRole("combobox", { name: "诊断记录" }));
    await user.click(await screen.findByRole("option", { name: /无法识别当前画面/ }));
    expect(screen.getByRole("textbox")).toHaveValue("第一条的草稿");
  });

  it("requires confirmation and removes only the selected record", async () => {
    const user = userEvent.setup();
    const second = { ...record, id: "record-2", reason: "第二次失败" };
    let records = [record, second];
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === "list_recognition_diagnostics") return records;
      if (cmd === "delete_recognition_diagnostic") records = [second];
      return null;
    });
    renderWithTheme(<RecognitionDiagnosticActions />);
    await user.click(await screen.findByRole("button", { name: "识别失败诊断" }));
    await user.click(screen.getByRole("button", { name: "删除当前记录" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("删除后无法恢复");
    expect(invoke).not.toHaveBeenCalledWith("delete_recognition_diagnostic", expect.anything());
    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(invoke).not.toHaveBeenCalledWith("delete_recognition_diagnostic", expect.anything());
    await user.click(screen.getByRole("button", { name: "删除当前记录" }));
    await user.click(screen.getByRole("button", { name: "确认删除" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("delete_recognition_diagnostic", { diagnosticId: record.id }));
    expect(await screen.findByRole("status")).toHaveTextContent("诊断记录已删除");
    expect(screen.getByRole("combobox", { name: "诊断记录" })).toHaveTextContent("第二次失败");
  });
  it("clears all records and keeps an empty state until the dialog closes", async () => {
    const user = userEvent.setup();
    let records = [record];
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === "list_recognition_diagnostics") return records;
      if (cmd === "clear_recognition_diagnostics") records = [];
      return null;
    });
    renderWithTheme(<RecognitionDiagnosticActions />);
    await user.click(await screen.findByRole("button", { name: "识别失败诊断" }));
    await user.click(screen.getByRole("button", { name: "清空全部记录" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("全部诊断记录");
    await user.click(screen.getByRole("button", { name: "确认清空" }));
    expect(await screen.findByText("暂无诊断记录")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "导出诊断包" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "关闭" }));
    expect(screen.queryByRole("button", { name: "识别失败诊断" })).not.toBeInTheDocument();
  });
  it("keeps the record and shows errors when deletion fails", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === "list_recognition_diagnostics") return [record];
      throw new Error("permission denied");
    });
    renderWithTheme(<RecognitionDiagnosticActions />);
    await user.click(await screen.findByRole("button", { name: "识别失败诊断" }));
    await user.click(screen.getByRole("button", { name: "删除当前记录" }));
    await user.click(screen.getByRole("button", { name: "确认删除" }));
    expect(await screen.findByRole("status")).toHaveTextContent("清除失败");
    expect(screen.getByRole("button", { name: "删除当前记录" })).toBeEnabled();
    expect(screen.getAllByRole("img")).toHaveLength(3);
  });

});
