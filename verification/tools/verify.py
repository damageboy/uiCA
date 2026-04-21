def first_mismatch_path(left, right, path="$"):
    if type(left) is not type(right):
        return path

    if isinstance(left, dict):
        keys = sorted(set(left.keys()) | set(right.keys()))
        for key in keys:
            if key not in left or key not in right:
                return f"{path}.{key}"

            mismatch = first_mismatch_path(left[key], right[key], f"{path}.{key}")
            if mismatch:
                return mismatch
        return None

    if isinstance(left, list):
        if len(left) != len(right):
            return f"{path}.length"

        for idx, (left_item, right_item) in enumerate(zip(left, right, strict=False)):
            mismatch = first_mismatch_path(left_item, right_item, f"{path}[{idx}]")
            if mismatch:
                return mismatch
        return None

    return None if left == right else path
