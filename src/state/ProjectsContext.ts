import { createContext, useContext } from "react";
import type { DiscoveryApi } from "../api/DiscoveryApi";
import type { ProjectRule } from "../types";
import type { Grouping } from "./clusters";
import type { similarNames } from "./similarNames";

export interface ProjectsValue extends Grouping {
  readonly rules: readonly ProjectRule[];
  readonly rulesError: string | null;
  readonly similar: typeof similarNames;
  readonly assignProject: DiscoveryApi["assignProject"];
}

/** Empty, so a component rendered without App (most tests) sees no projects and changes nothing. */
const NONE: ProjectsValue = {
  clusters: [],
  clusterOf: new Map(),
  tags: new Map(),
  unplaced: [],
  rules: [],
  rulesError: null,
  similar: () => [],
  assignProject: () => Promise.resolve(),
};

export const ProjectsContext = createContext<ProjectsValue>(NONE);

export const useProjects = (): ProjectsValue => useContext(ProjectsContext);
