// MergeRequestPage.tsx — Phase 1 stub for W-FE-11.

import { useEffect } from 'react';
import { useParams } from 'react-router-dom';

import { useSelectionStore } from '../stores/selectionStore';
import { useRealtime } from '../hooks/useRealtime';
import { StubPage } from './StubPage';

export function MergeRequestPage(): JSX.Element {
  const { iid } = useParams();
  const setMr = useSelectionStore((s) => s.setCurrentMr);

  useEffect(() => {
    if (!iid) return undefined;
    setMr(iid);
    return () => setMr(null);
  }, [iid, setMr]);

  useRealtime(iid ? [`mr.${iid}`] : []);

  return (
    <StubPage
      title={`Merge request !${iid ?? '?'}`}
      workPackage="W-FE-11"
      description="Three-pane diff cockpit. Phase 1 wires the selection store + WS scope so the dock updates on the right events."
    />
  );
}
