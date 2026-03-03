import { auth } from 'thepopebot/auth';
import { CodePage } from 'thepopebot/code';

export default async function CodeRoute({ params }) {
  const session = await auth();
  const { claudeWorkspaceId } = await params;
  return <CodePage session={session} claudeWorkspaceId={claudeWorkspaceId} />;
}
