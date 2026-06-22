// "@adminpanel" is resolved by a Vite alias (see vite.config.ts) to either the
// git-ignored real Admin tab (admin builds) or a harmless stub (player builds).
declare module "@adminpanel" {
  export const ADMIN_ENABLED: boolean;
  const AdminPanel: () => JSX.Element | null;
  export default AdminPanel;
}
