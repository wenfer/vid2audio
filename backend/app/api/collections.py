from __future__ import annotations

from fastapi import APIRouter, HTTPException

from backend.app.api.deps import get_collection_service

router = APIRouter(prefix="/collections", tags=["collections"])


@router.get("")
def list_collections():
    return get_collection_service().list_collections()


@router.get("/{collection_id}")
def get_collection(collection_id: str):
    collection = get_collection_service().get_collection(collection_id)
    if not collection:
        raise HTTPException(status_code=404, detail="合集不存在")
    return collection


@router.post("/{collection_id}/scan")
def rescan_collection(collection_id: str):
    service = get_collection_service()
    collection = service.get_collection(collection_id)
    if not collection:
        raise HTTPException(status_code=404, detail="合集不存在")
    return service.scan([collection.source_path])


@router.delete("/{collection_id}")
def delete_collection(collection_id: str):
    deleted = get_collection_service().delete_collection(collection_id)
    if not deleted:
        raise HTTPException(status_code=404, detail="合集不存在")
    return {"deleted": True}
