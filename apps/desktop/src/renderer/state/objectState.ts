import type { Dispatch, SetStateAction } from "react";

export type ObjectStateAction<State> =
  | { type: "field"; key: keyof State; updater: SetStateAction<State[keyof State]> }
  | { type: "patch"; patch: Partial<State> }
  | { type: "replace"; next: State };

export function resolveSetStateAction<Value>(
  current: Value,
  updater: SetStateAction<Value>,
): Value {
  return typeof updater === "function" ? (updater as (value: Value) => Value)(current) : updater;
}

export function objectStateReducer<State extends object>(
  state: State,
  action: ObjectStateAction<State>,
): State {
  switch (action.type) {
    case "field": {
      const nextValue = resolveSetStateAction(state[action.key], action.updater);
      if (Object.is(nextValue, state[action.key])) {
        return state;
      }
      return {
        ...state,
        [action.key]: nextValue,
      };
    }
    case "patch": {
      let changed = false;
      for (const [key, value] of Object.entries(action.patch) as Array<
        [keyof State, State[keyof State]]
      >) {
        if (!Object.is(state[key], value)) {
          changed = true;
          break;
        }
      }
      if (!changed) {
        return state;
      }
      return {
        ...state,
        ...action.patch,
      };
    }
    case "replace":
      return Object.is(action.next, state) ? state : action.next;
    default:
      return state;
  }
}

export function updateField<State extends object, Key extends keyof State>(
  dispatch: Dispatch<ObjectStateAction<State>>,
  key: Key,
  updater: SetStateAction<State[Key]>,
) {
  dispatch({
    type: "field",
    key,
    updater,
  } as ObjectStateAction<State>);
}
