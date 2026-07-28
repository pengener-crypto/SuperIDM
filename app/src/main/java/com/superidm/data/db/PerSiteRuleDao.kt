package com.superidm.data.db

import androidx.room.*
import kotlinx.coroutines.flow.Flow

@Dao
interface PerSiteRuleDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(entity: PerSiteRuleEntity)
    
    @Delete
    suspend fun delete(entity: PerSiteRuleEntity)
    
    @Query("SELECT * FROM per_site_rules ORDER BY createdAt DESC")
    fun getAll(): Flow<List<PerSiteRuleEntity>>
    
    @Query("SELECT * FROM per_site_rules WHERE domain = :domain LIMIT 1")
    suspend fun getByDomain(domain: String): PerSiteRuleEntity?
    
    @Query("DELETE FROM per_site_rules WHERE domain = :domain")
    suspend fun deleteByDomain(domain: String)
    
    @Query("SELECT * FROM per_site_rules WHERE :host LIKE '%' || domain ORDER BY LENGTH(domain) DESC LIMIT 1")
    suspend fun findMatchingRule(host: String): PerSiteRuleEntity?
}
