-- The inbox filer: a behavior rather than a command.
--
-- It declares WHEN it runs, and the review names that beside the rings it
-- asks for. Nothing invokes it: a node appearing under the watched folder is
-- what wakes it, and what it writes reads back attributed to it.
-- @watch file:///notes/
mere.open('mere://filed/' .. #mere.trigger())
