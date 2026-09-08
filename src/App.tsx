import { useCallback, useState } from "react";
import { Routes, Route, Navigate, useNavigate, useParams } from "react-router-dom";
import { AppShell } from "./components/AppShell/AppShell";
import { AppRail } from "./components/AppRail/AppRail";
import { LibraryEmptyState } from "./screens/Library/LibraryEmptyState";
import { Library } from "./screens/Library/Library";
import { Import } from "./screens/Import/Import";
import { ComingSoon } from "./screens/ComingSoon/ComingSoon";
import { Dashboard } from "./screens/Dashboard/Dashboard";
import { Participants } from "./screens/Participants/Participants";
import { useChats } from "./hooks/useChats";
import { useImportChat } from "./hooks/useImportChat";
import { useDeleteChat } from "./hooks/useDeleteChat";
import { usePeopleReview } from "./hooks/usePeopleReview";
import { ImportFlowOverlay } from "./components/ImportFlow/ImportFlowOverlay";
import { ParticipantReviewStep } from "./components/ImportFlow/ParticipantReviewStep";
import type { ChatSummary } from "./lib/types";

function DashboardRoute({ chats, onReimport }: { chats: ChatSummary[]; onReimport: () => void }) {
  const { chatId } = useParams();
  const id = Number(chatId);
  // Чат мог быть удалён из другой вкладки состояния (например, повторный
  // клик «Удалить» на LibraryCard, пока эта же ссылка ещё жила в рельсе)
  // — сюда мы попадаем, только когда id уже есть в списке (см. handleSelectChat/
  // handleImported), но список асинхронный, так что подстрахуемся.
  if (!chats.some((c) => c.id === id)) return <Navigate to="/" replace />;
  return <Dashboard chatId={id} onReimport={onReimport} />;
}

function ParticipantsRoute({ chats }: { chats: ChatSummary[] }) {
  const { chatId } = useParams();
  const id = Number(chatId);
  const chat = chats.find((c) => c.id === id);
  if (!chat) return <Navigate to="/" replace />;
  return <Participants chatId={id} chatName={chat.name} />;
}

function App() {
  const navigate = useNavigate();
  const [reviewChatId, setReviewChatId] = useState<number | null>(null);
  const { chats, refresh } = useChats();

  const handleImported = useCallback(
    async (chat: { id: number }) => {
      await refresh();
      // Шаг 3 мастера импорта (макет 3d) — разбор участников — перед тем,
      // как открыть дашборд, а не сразу.
      setReviewChatId(chat.id);
    },
    [refresh],
  );
  const { importFile, cancelImport, dismissError, isImporting, progress, error } = useImportChat({
    onImported: handleImported,
  });
  const { deleteChat } = useDeleteChat();
  const { people, merge, hide, rename } = usePeopleReview(reviewChatId);

  function handleSelectChat(id: number) {
    navigate(`/chats/${id}`);
  }

  async function handleDeleteChat(id: number) {
    await deleteChat(id);
    await refresh();
    setReviewChatId((current) => (current === id ? null : current));
  }

  function finishReview() {
    if (reviewChatId != null) navigate(`/chats/${reviewChatId}`);
    setReviewChatId(null);
  }

  return (
    <AppShell rail={<AppRail chats={chats} />}>
      <Routes>
        <Route
          path="/"
          element={
            chats.length > 0 ? (
              <Library
                chats={chats}
                onImport={importFile}
                isImporting={isImporting}
                error={error?.message ?? null}
                onSelectChat={handleSelectChat}
                onDelete={handleDeleteChat}
              />
            ) : (
              <LibraryEmptyState onPickFile={importFile} isImporting={isImporting} error={error?.message ?? null} />
            )
          }
        />
        <Route path="/chats/:chatId" element={<DashboardRoute chats={chats} onReimport={importFile} />} />
        <Route path="/chats/:chatId/participants" element={<ParticipantsRoute chats={chats} />} />
        <Route
          path="/import"
          element={<Import onPickFile={importFile} isImporting={isImporting} error={error?.message ?? null} />}
        />
        <Route path="/slides" element={<ComingSoon title="Слайды" />} />
      </Routes>
      <ImportFlowOverlay
        progress={progress}
        error={error}
        onCancel={cancelImport}
        onPickAnother={importFile}
        onDismiss={dismissError}
      />
      {reviewChatId != null && (
        <ParticipantReviewStep
          people={people}
          onMerge={merge}
          onHide={hide}
          onRename={rename}
          onDone={finishReview}
          onSkip={finishReview}
        />
      )}
    </AppShell>
  );
}

export default App;
