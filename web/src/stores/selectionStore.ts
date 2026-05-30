// selectionStore.ts — current navigation context (W-FE-05).
//
// Tracks the entity the user is currently inspecting so deeply-nested
// components do not have to drill props from the router context. The store
// is updated by the route loaders / page components, never by user input
// directly.

import { create } from 'zustand';

export interface SelectionState {
  currentRepoId: string | null;
  currentRef: string | null;
  currentPath: string | null;
  currentPrNumber: string | null;
  setCurrentRepo: (id: string | null) => void;
  setCurrentRef: (ref: string | null) => void;
  setCurrentPath: (path: string | null) => void;
  setCurrentPr: (prNumber: string | null) => void;
  clear: () => void;
}

export const useSelectionStore = create<SelectionState>((set) => ({
  currentRepoId: null,
  currentRef: null,
  currentPath: null,
  currentPrNumber: null,
  setCurrentRepo: (id) => set({ currentRepoId: id }),
  setCurrentRef: (ref) => set({ currentRef: ref }),
  setCurrentPath: (path) => set({ currentPath: path }),
  setCurrentPr: (prNumber) => set({ currentPrNumber: prNumber }),
  clear: () =>
    set({
      currentRepoId: null,
      currentRef: null,
      currentPath: null,
      currentPrNumber: null,
    }),
}));
