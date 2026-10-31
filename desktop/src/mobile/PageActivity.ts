import { createContext, useContext } from "react";

/** Cached routes retain their controls, but only the visible route polls. */
export const PageActivity = createContext(true);
export const usePageActive = () => useContext(PageActivity);
