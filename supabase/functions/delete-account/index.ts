// Full "delete my account" — removes the auth.users row itself (so the
// email can be reused and the person can never log back in as this
// account), not just their data. The Rust client's SupabaseStorage::
// delete_all_data (src/api/storage.rs) already handles the data-only path
// with zero deployment, using the existing owner-scoped RLS policies and
// no privileged key. Actually deleting the auth.users row needs the
// service-role key, which can only ever live here — never in the client.
//
// NOT deployed yet. To deploy (once, with the Supabase CLI installed and
// this project linked — `supabase link --project-ref <your-project-ref>`,
// run from the repo root):
//   supabase functions deploy delete-account
//
// No secrets need to be set manually — SUPABASE_URL and
// SUPABASE_SERVICE_ROLE_KEY are automatically available to every Edge
// Function on this platform, you don't provide them yourself.
//
// Call from the client with the user's own access token (there's
// deliberately no client-side call site wired up for this yet — add one
// once this is actually deployed, otherwise it's a button that 404s):
//   POST {SUPABASE_URL}/functions/v1/delete-account
//   Authorization: Bearer <user's access_token>

import { createClient } from "npm:@supabase/supabase-js@2";

Deno.serve(async (req: Request) => {
  if (req.method !== "POST") {
    return new Response("Method not allowed", { status: 405 });
  }

  const authHeader = req.headers.get("Authorization");
  if (!authHeader) {
    return new Response(JSON.stringify({ error: "Missing Authorization header" }), { status: 401 });
  }

  const supabaseUrl = Deno.env.get("SUPABASE_URL")!;
  const serviceRoleKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;

  // Verify the caller's own token first — this must only ever delete the
  // account making the request, never an arbitrary id passed in the body.
  const callerClient = createClient(supabaseUrl, serviceRoleKey, {
    global: { headers: { Authorization: authHeader } },
  });
  const { data: userData, error: userError } = await callerClient.auth.getUser();
  if (userError || !userData?.user) {
    return new Response(JSON.stringify({ error: "Invalid or expired session" }), { status: 401 });
  }
  const userId = userData.user.id;

  // Service-role client for the actual privileged operations below —
  // bypasses RLS entirely, so scope every query explicitly to userId.
  const adminClient = createClient(supabaseUrl, serviceRoleKey);

  // Data first (same tables/order as SupabaseStorage::delete_all_data in
  // the Rust client), then the auth.users row itself last.
  const { data: ownMoments } = await adminClient
    .from("moments")
    .select("id")
    .eq("user_id", userId);
  const momentIds = (ownMoments ?? []).map((m: { id: string }) => m.id);
  if (momentIds.length > 0) {
    await adminClient.from("reactions").delete().in("moment_id", momentIds);
  }
  await adminClient.from("moments").delete().eq("user_id", userId);
  await adminClient.from("entities").delete().eq("user_id", userId);

  const { error: deleteUserError } = await adminClient.auth.admin.deleteUser(userId);
  if (deleteUserError) {
    return new Response(JSON.stringify({ error: deleteUserError.message }), { status: 500 });
  }

  return new Response(JSON.stringify({ ok: true }), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
});
