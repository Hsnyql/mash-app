export interface RecognitionDiagnosticSummary {
  id: string;
  createdAtMs: number;
  reason: string;
  imagePaths: string[];
  missing: string[];
  userDescription: string;
}
